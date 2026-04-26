// Copyright (c) 2026 Judica, Inc.
// Distributed under the MIT software license, see the accompanying
// file COPYING or https://opensource.org/license/mit/.

//! Char subsystem limits — keep names and numeric values aligned with the C++ Char node so
//! reviewers can grep for drift against `src/char/util/constants.h` (`Char::constants`) and
//! RPC checks in `src/rpc/char_rpc.cpp`.

/// Maximum bamboo payload size in bytes.
///
/// **Must match** `Char::constants::MAX_CHAR_BAMBOO_SIZE` in `src/char/util/constants.h`.
pub const MAX_CHAR_BAMBOO_SIZE: usize = 3_000_000;

/// `getreferendumdecisionroll` rejects when `end_ballot - start_ballot >=` this value.
///
/// **Must match** local `MAX_RANGE` in `getreferendumdecisionroll` in `src/rpc/char_rpc.cpp`.
pub const GET_REFERENDUM_DECISION_ROLL_MAX_RANGE: u64 = 100;

/// Largest allowed `end_ballot - start_ballot` in a single `getreferendumdecisionroll` call.
///
/// Equals [`GET_REFERENDUM_DECISION_ROLL_MAX_RANGE`] minus one (node uses `>=` in the check).
pub const GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN: u64 =
    GET_REFERENDUM_DECISION_ROLL_MAX_RANGE - 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_roll_span_matches_node_rpc_rule() {
        assert_eq!(GET_REFERENDUM_DECISION_ROLL_MAX_RANGE, 100);
        assert_eq!(GET_REFERENDUM_DECISION_ROLL_MAX_INCLUSIVE_SPAN, 99);
    }
}
