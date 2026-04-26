// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Progress model: verified vs observed ballot; no silent rollback/gap.

use thiserror::Error;

/// Progress state: last confirmed/observed/finality ballot. Caller persists; SDK does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Last ballot for which we have confirmed outcome (e.g. fetched decision roll from Char).
    pub confirmed_ballot: Option<u64>,
    /// Last ballot we've observed (e.g. from ZMQ) not yet verified.
    pub observed_ballot: Option<u64>,
    /// Last ballot considered final (e.g. by L1 confirmations); optional.
    pub finality_ballot: Option<u64>,
}

impl Progress {
    /// Next ballot number to verify (confirmed_ballot + 1, or 0 if none).
    pub fn next_ballot_to_verify(&self) -> u64 {
        self.confirmed_ballot.map(|b| b + 1).unwrap_or(0)
    }

    /// Advance verified progress to this ballot. Fails on rollback or gap.
    pub fn advance_verified(&mut self, ballot: u64) -> Result<(), ProgressError> {
        let expected = self.next_ballot_to_verify();
        if ballot != expected {
            return Err(ProgressError::RollbackOrGap {
                expected,
                got: ballot,
            });
        }
        if ballot == u64::MAX {
            return Err(ProgressError::Overflow);
        }
        self.confirmed_ballot = Some(ballot);
        Ok(())
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ProgressError {
    #[error("rollback or gap: expected next ballot {expected}, got {got}")]
    RollbackOrGap { expected: u64, got: u64 },

    #[error("ballot overflow")]
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_ballot_to_verify_none() {
        let p = Progress::default();
        assert_eq!(p.next_ballot_to_verify(), 0);
    }

    #[test]
    fn next_ballot_to_verify_some() {
        let p = Progress {
            confirmed_ballot: Some(5),
            observed_ballot: None,
            finality_ballot: None,
        };
        assert_eq!(p.next_ballot_to_verify(), 6);
    }

    #[test]
    fn advance_verified_sequential() {
        let mut p = Progress::default();
        assert!(p.advance_verified(0).is_ok());
        assert_eq!(p.confirmed_ballot, Some(0));
        assert!(p.advance_verified(1).is_ok());
        assert_eq!(p.confirmed_ballot, Some(1));
    }

    #[test]
    fn advance_verified_gap_err() {
        let mut p = Progress::default();
        p.advance_verified(0).unwrap();
        let e = p.advance_verified(2).unwrap_err();
        assert!(matches!(
            e,
            ProgressError::RollbackOrGap {
                expected: 1,
                got: 2
            }
        ));
    }

    #[test]
    fn advance_verified_rollback_err() {
        let mut p = Progress::default();
        p.advance_verified(0).unwrap();
        p.advance_verified(1).unwrap();
        let e = p.advance_verified(0).unwrap_err();
        assert!(matches!(
            e,
            ProgressError::RollbackOrGap {
                expected: 2,
                got: 0
            }
        ));
    }
}
