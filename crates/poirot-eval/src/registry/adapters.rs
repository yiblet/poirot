//! Explicit per-family helper membership. The macro expands to exhaustive typed matches;
//! `local` variants stay in evaluator-owned execution paths.
use super::{DatabaseFunction, ReferenceFunction, SearchFunction};

macro_rules! helper_map {
    (total $family:ident => $target:ty; helpers: [$($helper:ident),+ $(,)?];) => {
        impl $family {
            /// Map this builtin identity to its family helper.
            pub(crate) fn helper(self) -> $target {
                match self {
                    $(Self::$helper => <$target>::$helper,)+
                }
            }
        }
    };
    (partial $family:ident => $target:ty;
        helpers: [$($helper:ident),+ $(,)?];
        local: [$($local:ident),+ $(,)?];) => {
        impl $family {
            /// Return the family helper, or `None` for evaluator-owned handlers.
            pub(crate) fn helper(self) -> Option<$target> {
                match self {
                    $(Self::$helper => Some(<$target>::$helper),)+
                    $(Self::$local)|+ => None,
                }
            }
        }
    };
    (wrapped $family:ident => $outer:ty, $inner:ty, $wrap:ident;
        helpers: [$($helper:ident),+ $(,)?];
        local: [$($local:ident),+ $(,)?];) => {
        impl $family {
            /// Wrap a family helper, or return `None` for evaluator-owned handlers.
            pub(crate) fn helper(self) -> Option<$outer> {
                match self {
                    $(Self::$helper => Some(<$outer>::$wrap(<$inner>::$helper)),)+
                    $(Self::$local)|+ => None,
                }
            }
        }
    };
}

helper_map! {
    total DatabaseFunction => poirot_functions_database::DatabaseFunction;
    helpers: [
        Average, Count, CountA, Get, Max,
        Min, Product, Stdev, StdevP, Sum,
        Var, VarP,
    ];
}

helper_map! {
    wrapped SearchFunction => poirot_functions_lookup::Function, poirot_functions_lookup::Search, Search;
    helpers: [
        Lookup, XLookup, XMatch,
    ];
    local: [
        Match, VLookup, HLookup,
    ];
}

helper_map! {
    wrapped ReferenceFunction => poirot_functions_lookup::Function, poirot_functions_lookup::ReferenceFunction, Reference;
    helpers: [
        Indirect,
    ];
    local: [
        Address, Areas, Hyperlink, Sheet, Rows, Columns, Row, Column, Index, AnchorArray, Single, Offset,
    ];
}
