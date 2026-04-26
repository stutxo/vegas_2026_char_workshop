use crate::{
    BitcoinBlobLocator, BlobWriteReceipt, BorshBundle, BorshPayload, CouncilBlobLocator, DaError,
    DaKey, DaMember, DaVerifyReport, DataAvailability, DataAvailabilityExt, LiquidBlobLocator,
    Locator, MemberId, MultiDa, Policy, PolicyKey, PolicySpec, RuntimeError, SemanticError,
    UsageError, WriteIncomplete, encode_borsh,
};
use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};
use std::sync::Arc;

#[derive(Clone)]
struct MockBackend {
    member_id: MemberId,
    provider_kind: &'static str,
    write_result: Result<Locator, DaError>,
    read_result: Result<Vec<u8>, DaError>,
    verify_result: Option<Result<DaVerifyReport, DaError>>,
}

#[async_trait]
impl DataAvailability for MockBackend {
    fn provider_kind(&self) -> &'static str {
        self.provider_kind
    }

    fn member_id(&self) -> MemberId {
        self.member_id.clone()
    }

    async fn write_blob(&self, data: &[u8]) -> Result<BlobWriteReceipt, DaError> {
        let locator = self.write_result.clone()?;
        Ok(BlobWriteReceipt::new(
            PolicyKey::leaf(self.member_id.clone(), locator),
            data.len(),
        ))
    }

    async fn read_blob(&self, _key: &dyn DaKey) -> Result<Vec<u8>, DaError> {
        self.read_result.clone()
    }

    async fn verify_key(&self, key: &dyn DaKey) -> Result<DaVerifyReport, DaError> {
        if let Some(result) = &self.verify_result {
            return result.clone();
        }

        let _ = self.read_blob(key).await?;
        Ok(DaVerifyReport::new(true, None))
    }

    fn decode_key(&self, locator: &Locator) -> Result<crate::DynKey, DaError> {
        match locator.provider_kind() {
            "bitcoin" => {
                let key = serde_json::from_slice::<BitcoinBlobLocator>(locator.key_bytes())
                    .map_err(|err| UsageError::BadLocator(err.to_string()))?;
                Ok(Arc::new(key))
            }
            "liquid" => {
                let key = serde_json::from_slice::<LiquidBlobLocator>(locator.key_bytes())
                    .map_err(|err| UsageError::BadLocator(err.to_string()))?;
                Ok(Arc::new(key))
            }
            "council" => {
                let key = serde_json::from_slice::<CouncilBlobLocator>(locator.key_bytes())
                    .map_err(|err| UsageError::BadLocator(err.to_string()))?;
                Ok(Arc::new(key))
            }
            other => Err(UsageError::WrongProvider {
                expected: match other {
                    "bitcoin" => "bitcoin",
                    "liquid" => "liquid",
                    _ => "council",
                },
            }
            .into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ExampleBorshPayload {
    ballot: u64,
    payload: Vec<u8>,
}

impl BorshPayload for ExampleBorshPayload {}

fn bitcoin_locator(bytes: [u8; 32]) -> Locator {
    Locator::from_key(&BitcoinBlobLocator::new(bytes)).expect("bitcoin locator should encode")
}

fn liquid_locator(bytes: [u8; 32]) -> Locator {
    Locator::from_key(&LiquidBlobLocator::new(bytes)).expect("liquid locator should encode")
}

fn council_locator(bytes: [u8; 32]) -> Locator {
    Locator::from_key(&CouncilBlobLocator::new(bytes)).expect("council locator should encode")
}

fn default_council_member() -> MemberId {
    MemberId::council("default").expect("council label should be valid")
}

fn example_policy() -> Policy {
    Policy::And(vec![
        Policy::Leaf(MemberId::Bitcoin),
        Policy::Leaf(MemberId::Liquid),
        Policy::Leaf(default_council_member()),
    ])
}

#[tokio::test]
async fn leaf_data_availability_ext_round_trips_typed_payload() {
    let payload = ExampleBorshPayload {
        ballot: 7,
        payload: b"decision-roll".to_vec(),
    };
    let encoded = encode_borsh(&payload).expect("payload should encode");
    let da = MockBackend {
        member_id: MemberId::Bitcoin,
        provider_kind: "bitcoin",
        write_result: Ok(bitcoin_locator([0x55; 32])),
        read_result: Ok(encoded.clone()),
        verify_result: None,
    };

    let receipt = da.write(&payload).await.expect("write should succeed");
    assert_eq!(receipt.size(), encoded.len());
    assert!(matches!(receipt.key(), PolicyKey::Leaf(_)));

    let decoded: ExampleBorshPayload = da.read(receipt.key()).await.expect("read should succeed");
    assert_eq!(decoded, payload);

    let verify = da
        .verify(receipt.key())
        .await
        .expect("verify should succeed");
    assert!(verify.is_read_guaranteed());
}

#[tokio::test]
async fn leaf_data_availability_ext_rejects_other_member_key() {
    let east = MemberId::council("east").expect("council label should be valid");
    let west = MemberId::council("west").expect("council label should be valid");
    let da = MockBackend {
        member_id: east.clone(),
        provider_kind: "council",
        write_result: Ok(council_locator([0x55; 32])),
        read_result: Ok(b"hello".to_vec()),
        verify_result: Some(Ok(DaVerifyReport::new(true, Some("ok".to_string())))),
    };
    let key = PolicyKey::leaf(west, council_locator([0x55; 32]));

    match da.verify(&key).await {
        Err(DaError::Usage(UsageError::BadPolicyKey(message))) => {
            assert!(message.contains("expected member"));
        }
        other => panic!("expected member mismatch error, got {other:?}"),
    }
}

#[tokio::test]
async fn multi_da_all_of_derives_and_policy_from_members() {
    let da = MultiDa::all_of(vec![
        MockBackend {
            member_id: MemberId::Bitcoin,
            provider_kind: "bitcoin",
            write_result: Ok(bitcoin_locator([0x11; 32])),
            read_result: Err(SemanticError::NotFound.into()),
            verify_result: None,
        },
        MockBackend {
            member_id: MemberId::Liquid,
            provider_kind: "liquid",
            write_result: Ok(liquid_locator([0x33; 32])),
            read_result: Err(SemanticError::NotFound.into()),
            verify_result: None,
        },
    ])
    .expect("all_of should build");

    assert_eq!(
        da.policy(),
        &Policy::And(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::Leaf(MemberId::Liquid),
        ])
    );
}

#[tokio::test]
async fn multi_da_any_of_derives_or_policy_from_members() {
    let da = MultiDa::any_of(vec![
        MockBackend {
            member_id: MemberId::Bitcoin,
            provider_kind: "bitcoin",
            write_result: Ok(bitcoin_locator([0x11; 32])),
            read_result: Err(SemanticError::NotFound.into()),
            verify_result: None,
        },
        MockBackend {
            member_id: default_council_member(),
            provider_kind: "council",
            write_result: Ok(council_locator([0x22; 32])),
            read_result: Err(SemanticError::NotFound.into()),
            verify_result: None,
        },
    ])
    .expect("any_of should build");

    assert_eq!(
        da.policy(),
        &Policy::Or(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::Leaf(default_council_member()),
        ])
    );
}

#[tokio::test]
async fn multi_da_from_spec_builds_nested_policy_from_members() {
    let da = MultiDa::from_spec(PolicySpec::Or(vec![
        PolicySpec::Leaf(DaMember::bitcoin(MockBackend {
            member_id: MemberId::Bitcoin,
            provider_kind: "bitcoin",
            write_result: Ok(bitcoin_locator([0x11; 32])),
            read_result: Err(SemanticError::NotFound.into()),
            verify_result: None,
        })),
        PolicySpec::And(vec![
            PolicySpec::Leaf(DaMember::liquid(MockBackend {
                member_id: MemberId::Liquid,
                provider_kind: "liquid",
                write_result: Ok(liquid_locator([0x33; 32])),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            })),
            PolicySpec::Leaf(DaMember::new(
                default_council_member(),
                MockBackend {
                    member_id: default_council_member(),
                    provider_kind: "council",
                    write_result: Ok(council_locator([0x22; 32])),
                    read_result: Err(SemanticError::NotFound.into()),
                    verify_result: None,
                },
            )),
        ]),
    ]))
    .expect("from_spec should build");

    assert_eq!(
        da.policy(),
        &Policy::Or(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::And(vec![
                Policy::Leaf(MemberId::Liquid),
                Policy::Leaf(default_council_member()),
            ]),
        ])
    );
}

#[tokio::test]
async fn multi_da_from_macro_builds_nested_policy_from_members() {
    let da = MultiDa::from_spec(crate::policy_spec!(any(
        MockBackend {
            member_id: MemberId::Bitcoin,
            provider_kind: "bitcoin",
            write_result: Ok(bitcoin_locator([0x11; 32])),
            read_result: Err(SemanticError::NotFound.into()),
            verify_result: None,
        },
        all(
            MockBackend {
                member_id: MemberId::Liquid,
                provider_kind: "liquid",
                write_result: Ok(liquid_locator([0x33; 32])),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            },
            MockBackend {
                member_id: default_council_member(),
                provider_kind: "council",
                write_result: Ok(council_locator([0x22; 32])),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            },
        ),
    )))
    .expect("macro should build");

    assert_eq!(
        da.policy(),
        &Policy::Or(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::And(vec![
                Policy::Leaf(MemberId::Liquid),
                Policy::Leaf(default_council_member()),
            ]),
        ])
    );
}

#[tokio::test]
async fn multi_da_rejects_unknown_member_referenced_by_policy() {
    let result = MultiDa::new(
        vec![DaMember::bitcoin(MockBackend {
            member_id: MemberId::Bitcoin,
            provider_kind: "bitcoin",
            write_result: Ok(bitcoin_locator([0x11; 32])),
            read_result: Err(SemanticError::NotFound.into()),
            verify_result: None,
        })],
        Policy::And(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::Leaf(MemberId::Liquid),
        ]),
    );

    match result {
        Err(DaError::Usage(UsageError::UnknownMember(MemberId::Liquid))) => {}
        Ok(_) => panic!("expected unknown member error, got success"),
        Err(err) => panic!("expected unknown member error, got {err:?}"),
    }
}

#[tokio::test]
async fn multi_da_rejects_member_registry_entry_that_disagrees_with_backend() {
    let result = MultiDa::new(
        vec![DaMember::new(
            MemberId::Bitcoin,
            MockBackend {
                member_id: MemberId::Liquid,
                provider_kind: "liquid",
                write_result: Ok(liquid_locator([0x33; 32])),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            },
        )],
        Policy::Leaf(MemberId::Bitcoin),
    );

    match result {
        Err(DaError::Usage(UsageError::InvalidComposition(message))) => {
            assert!(message.contains("does not match backend member"));
        }
        Ok(_) => panic!("expected invalid composition error, got success"),
        Err(err) => panic!("expected invalid composition error, got {err:?}"),
    }
}

#[tokio::test]
async fn multi_da_from_spec_rejects_empty_and() {
    match MultiDa::from_spec(PolicySpec::And(vec![])) {
        Err(DaError::Usage(UsageError::InvalidComposition(_))) => {}
        Ok(_) => panic!("expected invalid composition error, got success"),
        Err(err) => panic!("expected invalid composition error, got {err:?}"),
    }
}

#[tokio::test]
async fn multi_da_and_policy_persists_one_leaf_key_per_branch() {
    let da = MultiDa::new(
        vec![
            DaMember::bitcoin(MockBackend {
                member_id: MemberId::Bitcoin,
                provider_kind: "bitcoin",
                write_result: Ok(bitcoin_locator([0x11; 32])),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            }),
            DaMember::liquid(MockBackend {
                member_id: MemberId::Liquid,
                provider_kind: "liquid",
                write_result: Ok(liquid_locator([0x33; 32])),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            }),
            DaMember::new(
                default_council_member(),
                MockBackend {
                    member_id: default_council_member(),
                    provider_kind: "council",
                    write_result: Ok(council_locator([0x22; 32])),
                    read_result: Err(SemanticError::NotFound.into()),
                    verify_result: None,
                },
            ),
        ],
        example_policy(),
    )
    .expect("policy and members should be valid");

    let payload = ExampleBorshPayload {
        ballot: 7,
        payload: b"decision-roll".to_vec(),
    };
    let receipt = da.write(&payload).await.expect("write should succeed");

    let PolicyKey::And(children) = receipt.key() else {
        panic!("AND policy should return PolicyKey::And");
    };
    assert_eq!(children.len(), 3);
    assert!(matches!(children[0], PolicyKey::Leaf(_)));
    assert!(matches!(children[1], PolicyKey::Leaf(_)));
    assert!(matches!(children[2], PolicyKey::Leaf(_)));
}

#[tokio::test]
async fn multi_da_rejects_malformed_policy_key_shape() {
    let da = MultiDa::all_of(vec![
        DaMember::bitcoin(MockBackend {
            member_id: MemberId::Bitcoin,
            provider_kind: "bitcoin",
            write_result: Ok(bitcoin_locator([0x11; 32])),
            read_result: Ok(b"hello".to_vec()),
            verify_result: None,
        }),
        DaMember::liquid(MockBackend {
            member_id: MemberId::Liquid,
            provider_kind: "liquid",
            write_result: Ok(liquid_locator([0x33; 32])),
            read_result: Ok(b"hello".to_vec()),
            verify_result: None,
        }),
    ])
    .expect("all_of should build");

    let bad_key = PolicyKey::Or(vec![PolicyKey::leaf(
        MemberId::Bitcoin,
        bitcoin_locator([0x11; 32]),
    )]);
    match da.read_blob(&bad_key).await {
        Err(DaError::Usage(UsageError::BadPolicyKey(_))) => {}
        other => panic!("expected bad policy key error, got {other:?}"),
    }
}

#[tokio::test]
async fn multi_da_rejects_policy_key_leaf_with_wrong_locator_kind() {
    let da = MultiDa::all_of(vec![
        DaMember::bitcoin(MockBackend {
            member_id: MemberId::Bitcoin,
            provider_kind: "bitcoin",
            write_result: Ok(bitcoin_locator([0x11; 32])),
            read_result: Ok(b"hello".to_vec()),
            verify_result: None,
        }),
        DaMember::liquid(MockBackend {
            member_id: MemberId::Liquid,
            provider_kind: "liquid",
            write_result: Ok(liquid_locator([0x33; 32])),
            read_result: Ok(b"hello".to_vec()),
            verify_result: None,
        }),
    ])
    .expect("all_of should build");

    let bad_key = PolicyKey::And(vec![
        PolicyKey::leaf(MemberId::Bitcoin, liquid_locator([0x33; 32])),
        PolicyKey::leaf(MemberId::Liquid, liquid_locator([0x44; 32])),
    ]);
    match da.read_blob(&bad_key).await {
        Err(DaError::Usage(UsageError::MemberLocatorMismatch {
            member_id,
            locator_kind,
        })) => {
            assert_eq!(member_id, MemberId::Bitcoin);
            assert_eq!(locator_kind, "liquid");
        }
        other => panic!("expected bad policy key error, got {other:?}"),
    }
}

#[tokio::test]
async fn multi_da_and_read_falls_back_to_first_readable_branch() {
    let payload = ExampleBorshPayload {
        ballot: 7,
        payload: b"decision-roll".to_vec(),
    };
    let encoded = encode_borsh(&payload).expect("payload should encode");

    let da =
        MultiDa::new(
            vec![
                DaMember::bitcoin(MockBackend {
                    member_id: MemberId::Bitcoin,
                    provider_kind: "bitcoin",
                    write_result: Ok(bitcoin_locator([0x11; 32])),
                    read_result: Err(SemanticError::NotFound.into()),
                    verify_result: None,
                }),
                DaMember::liquid(MockBackend {
                    member_id: MemberId::Liquid,
                    provider_kind: "liquid",
                    write_result: Ok(liquid_locator([0x33; 32])),
                    read_result: Err(
                        RuntimeError::ServiceUnavailable("liquid down".to_string()).into()
                    ),
                    verify_result: None,
                }),
                DaMember::new(
                    default_council_member(),
                    MockBackend {
                        member_id: default_council_member(),
                        provider_kind: "council",
                        write_result: Ok(council_locator([0x22; 32])),
                        read_result: Ok(encoded.clone()),
                        verify_result: Some(Ok(DaVerifyReport::new(
                            true,
                            Some("council available".to_string()),
                        ))),
                    },
                ),
            ],
            example_policy(),
        )
        .expect("policy and members should be valid");

    let receipt = da.write(&payload).await.expect("write should succeed");
    let decoded: ExampleBorshPayload = da.read(receipt.key()).await.expect("read should succeed");
    assert_eq!(decoded, payload);

    match da.verify(receipt.key()).await {
        Err(DaError::Semantic(SemanticError::NotFound)) => {}
        other => panic!("expected AND verify to fail on unreadable branches, got {other:?}"),
    }
}

#[tokio::test]
async fn multi_da_or_write_chooses_first_successful_branch() {
    let da = MultiDa::new(
        vec![
            DaMember::bitcoin(MockBackend {
                member_id: MemberId::Bitcoin,
                provider_kind: "bitcoin",
                write_result: Err(RuntimeError::ServiceUnavailable("btc down".to_string()).into()),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            }),
            DaMember::liquid(MockBackend {
                member_id: MemberId::Liquid,
                provider_kind: "liquid",
                write_result: Ok(liquid_locator([0x33; 32])),
                read_result: Ok(b"hello".to_vec()),
                verify_result: Some(Ok(DaVerifyReport::new(true, Some("liquid ok".to_string())))),
            }),
            DaMember::new(
                default_council_member(),
                MockBackend {
                    member_id: default_council_member(),
                    provider_kind: "council",
                    write_result: Err(UsageError::InvalidRequest(
                        "should not be called".to_string(),
                    )
                    .into()),
                    read_result: Ok(b"hello".to_vec()),
                    verify_result: Some(Ok(DaVerifyReport::new(
                        true,
                        Some("council ok".to_string()),
                    ))),
                },
            ),
        ],
        Policy::Or(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::Leaf(MemberId::Liquid),
            Policy::Leaf(default_council_member()),
        ]),
    )
    .expect("policy and members should be valid");

    let receipt = da.write_blob(b"hello").await.expect("write should succeed");
    let PolicyKey::Or(children) = receipt.key() else {
        panic!("OR policy should return PolicyKey::Or");
    };
    assert_eq!(children.len(), 1);
    assert_eq!(
        children[0],
        PolicyKey::leaf(MemberId::Liquid, liquid_locator([0x33; 32]))
    );
}

#[tokio::test]
async fn multi_da_or_distinguishes_same_provider_members() {
    let east = MemberId::council("east").expect("council label should be valid");
    let west = MemberId::council("west").expect("council label should be valid");
    let da = MultiDa::new(
        vec![
            DaMember::new(
                east.clone(),
                MockBackend {
                    member_id: east.clone(),
                    provider_kind: "council",
                    write_result: Err(
                        RuntimeError::ServiceUnavailable("east down".to_string()).into()
                    ),
                    read_result: Ok(b"east".to_vec()),
                    verify_result: Some(Ok(DaVerifyReport::new(true, Some("east ok".to_string())))),
                },
            ),
            DaMember::new(
                west.clone(),
                MockBackend {
                    member_id: west.clone(),
                    provider_kind: "council",
                    write_result: Ok(council_locator([0x77; 32])),
                    read_result: Ok(b"west".to_vec()),
                    verify_result: Some(Ok(DaVerifyReport::new(true, Some("west ok".to_string())))),
                },
            ),
        ],
        Policy::Or(vec![Policy::Leaf(east.clone()), Policy::Leaf(west.clone())]),
    )
    .expect("policy and members should be valid");

    let receipt = da.write_blob(b"hello").await.expect("write should succeed");
    assert_eq!(
        receipt.key(),
        &PolicyKey::Or(vec![PolicyKey::leaf(
            west.clone(),
            council_locator([0x77; 32]),
        )]),
    );

    let bytes = da
        .read_blob(receipt.key())
        .await
        .expect("read should succeed");
    assert_eq!(bytes, b"west".to_vec());
}

#[tokio::test]
async fn multi_da_or_read_exhaustion_reports_chosen_branch_summary() {
    let da = MultiDa::new(
        vec![
            DaMember::bitcoin(MockBackend {
                member_id: MemberId::Bitcoin,
                provider_kind: "bitcoin",
                write_result: Err(RuntimeError::ServiceUnavailable("btc down".to_string()).into()),
                read_result: Err(RuntimeError::ServiceUnavailable("btc down".to_string()).into()),
                verify_result: None,
            }),
            DaMember::new(
                default_council_member(),
                MockBackend {
                    member_id: default_council_member(),
                    provider_kind: "council",
                    write_result: Ok(council_locator([0x22; 32])),
                    read_result: Err(SemanticError::NotFound.into()),
                    verify_result: None,
                },
            ),
        ],
        Policy::Or(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::Leaf(default_council_member()),
        ]),
    )
    .expect("policy and members should be valid");

    let receipt = da.write_blob(b"hello").await.expect("write should succeed");
    match da.read_blob(receipt.key()).await {
        Err(DaError::Semantic(SemanticError::UnavailableAcrossPolicy(summary))) => {
            assert_eq!(summary.runtime_failures(), 0);
            assert_eq!(summary.not_found_failures(), 1);
            assert_eq!(summary.semantic_failures(), 0);
            assert_eq!(summary.usage_failures(), 0);
        }
        other => panic!("expected aggregate exhaustion, got {other:?}"),
    }
}

#[tokio::test]
async fn multi_da_or_hard_semantic_failure_on_chosen_branch_surfaces() {
    let da = MultiDa::new(
        vec![
            DaMember::bitcoin(MockBackend {
                member_id: MemberId::Bitcoin,
                provider_kind: "bitcoin",
                write_result: Ok(bitcoin_locator([0x11; 32])),
                read_result: Err(SemanticError::IntegrityFailure.into()),
                verify_result: None,
            }),
            DaMember::new(
                default_council_member(),
                MockBackend {
                    member_id: default_council_member(),
                    provider_kind: "council",
                    write_result: Ok(council_locator([0x22; 32])),
                    read_result: Ok(b"hello".to_vec()),
                    verify_result: Some(Ok(DaVerifyReport::new(
                        true,
                        Some("council ok".to_string()),
                    ))),
                },
            ),
        ],
        Policy::Or(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::Leaf(default_council_member()),
        ]),
    )
    .expect("policy and members should be valid");

    let receipt = da.write_blob(b"hello").await.expect("write should succeed");
    match da.read_blob(receipt.key()).await {
        Err(DaError::Semantic(SemanticError::IntegrityFailure)) => {}
        other => panic!("expected hard semantic failure to stop fallback, got {other:?}"),
    }
}

#[tokio::test]
async fn multi_da_and_write_returns_partial_key_on_late_failure() {
    let da = MultiDa::new(
        vec![
            DaMember::bitcoin(MockBackend {
                member_id: MemberId::Bitcoin,
                provider_kind: "bitcoin",
                write_result: Ok(bitcoin_locator([0x11; 32])),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            }),
            DaMember::liquid(MockBackend {
                member_id: MemberId::Liquid,
                provider_kind: "liquid",
                write_result: Err(
                    RuntimeError::ServiceUnavailable("liquid down".to_string()).into()
                ),
                read_result: Err(SemanticError::NotFound.into()),
                verify_result: None,
            }),
        ],
        Policy::And(vec![
            Policy::Leaf(MemberId::Bitcoin),
            Policy::Leaf(MemberId::Liquid),
        ]),
    )
    .expect("policy and members should be valid");

    match da.write_blob(b"hello").await {
        Err(DaError::Semantic(SemanticError::WriteIncomplete(WriteIncomplete { .. }))) => {}
        other => panic!("expected partial write error, got {other:?}"),
    }
}

#[tokio::test]
async fn generic_borsh_bundle_round_trips_through_leaf_backend() {
    let bundle = BorshBundle::new(vec![
        ExampleBorshPayload {
            ballot: 1,
            payload: b"one".to_vec(),
        },
        ExampleBorshPayload {
            ballot: 2,
            payload: b"two".to_vec(),
        },
    ])
    .expect("bundle should build");
    let encoded = bundle.encode().expect("bundle should encode");
    let da = MockBackend {
        member_id: default_council_member(),
        provider_kind: "council",
        write_result: Ok(council_locator([0x66; 32])),
        read_result: Ok(encoded.clone()),
        verify_result: None,
    };

    let receipt = da.write(&bundle).await.expect("write should succeed");
    let decoded: BorshBundle<ExampleBorshPayload> =
        da.read(receipt.key()).await.expect("read should succeed");
    assert_eq!(decoded, bundle);
}
