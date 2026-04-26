/// Build nested [`PolicySpec`] trees using `all(...)`, `any(...)`, and leaf expressions.
///
/// Any expression that is not wrapped in `all(...)` or `any(...)` is treated as a leaf member.
///
/// ```rust,no_run
/// use chomp::{CouncilDa, DaError, MultiDa, policy_spec};
///
/// fn build_multi(council: CouncilDa) -> Result<MultiDa, DaError> {
///     MultiDa::from_spec(policy_spec!(any(council)))
/// }
/// ```
#[macro_export]
macro_rules! policy_spec {
    (all()) => {
        compile_error!("policy_spec!(all(...)) requires at least one branch")
    };
    (any()) => {
        compile_error!("policy_spec!(any(...)) requires at least one branch")
    };
    (all($($branches:tt)*)) => {
        $crate::PolicySpec::And($crate::policy_spec!(@branches [] $($branches)*))
    };
    (any($($branches:tt)*)) => {
        $crate::PolicySpec::Or($crate::policy_spec!(@branches [] $($branches)*))
    };
    ($leaf:expr) => {
        $crate::PolicySpec::Leaf(($leaf).into())
    };

    (@branches [$($acc:expr,)*]) => {
        vec![$($acc,)*]
    };
    (@branches [$($acc:expr,)*] all($($inner:tt)*) $(, $($rest:tt)*)?) => {
        $crate::policy_spec!(
            @branches
            [
                $($acc,)*
                $crate::policy_spec!(all($($inner)*)),
            ]
            $($($rest)*)?
        )
    };
    (@branches [$($acc:expr,)*] any($($inner:tt)*) $(, $($rest:tt)*)?) => {
        $crate::policy_spec!(
            @branches
            [
                $($acc,)*
                $crate::policy_spec!(any($($inner)*)),
            ]
            $($($rest)*)?
        )
    };
    (@branches [$($acc:expr,)*] $leaf:expr $(, $($rest:tt)*)?) => {
        $crate::policy_spec!(
            @branches
            [
                $($acc,)*
                $crate::PolicySpec::Leaf(($leaf).into()),
            ]
            $($($rest)*)?
        )
    };
}
