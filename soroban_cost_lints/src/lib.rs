#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint_and_help;
use clippy_utils::get_enclosing_loop_or_multi_call_closure;
use rustc_hir as hir;
use rustc_lint::{LateContext, LateLintPass, LintStore};
use rustc_span::def_id::DefId;

dylint_linting::dylint_library!();

fn match_soroban_def_path<'tcx>(cx: &LateContext<'tcx>, def_id: DefId, segments: &[&str]) -> bool {
    let full = cx.tcx.def_path_str(def_id);
    let suffix: String = segments.join("::");
    full.ends_with(&suffix)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintCategory {
    StorageOperations,
    Compute,
    Memory,
    EntryLifecycle,
}

pub struct LintMetadata {
    pub lint: &'static rustc_lint::Lint,
    pub category: LintCategory,
}

pub const LINT_METADATA: &[LintMetadata] = &[
    LintMetadata {
        lint: SOROBAN_STORAGE_IN_LOOP,
        category: LintCategory::StorageOperations,
    },
    LintMetadata {
        lint: REDUNDANT_ENV_CLONE,
        category: LintCategory::Memory,
    },
    LintMetadata {
        lint: UNNECESSARY_HOST_FUNCTION_CALL,
        category: LintCategory::Compute,
    },
    LintMetadata {
        lint: HOST_IN_LOOP,
        category: LintCategory::Compute,
    },
    LintMetadata {
        lint: COLLECTION_LEN_IN_LOOP_CONDITION,
        category: LintCategory::Compute,
    },
];

#[unsafe(no_mangle)]
pub fn register_lints(_sess: &rustc_session::Session, lint_store: &mut LintStore) {
    lint_store.register_lints(&[
        SOROBAN_STORAGE_IN_LOOP,
        REDUNDANT_ENV_CLONE,
        UNNECESSARY_HOST_FUNCTION_CALL,
        HOST_IN_LOOP,
        COLLECTION_LEN_IN_LOOP_CONDITION,
    ]);
    lint_store.register_late_pass(|_| Box::new(SorobanStorageInLoop));
    lint_store.register_late_pass(|_| Box::new(RedundantEnvClone));
    lint_store.register_late_pass(|_| Box::new(UnnecessaryHostFunctionCall));
    lint_store.register_late_pass(|_| Box::new(HostInLoop));
    lint_store.register_late_pass(|_| Box::new(CollectionLenInLoopCondition));
}

rustc_session::declare_lint! {
    pub SOROBAN_STORAGE_IN_LOOP,
    Warn,
    "storage operations inside a loop"
}
pub struct SorobanStorageInLoop;
rustc_session::impl_lint_pass!(SorobanStorageInLoop => [SOROBAN_STORAGE_IN_LOOP]);

impl<'tcx> LateLintPass<'tcx> for SorobanStorageInLoop {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::MethodCall(path_segment, receiver, _args, _span) = expr.kind {
            let receiver_ty = cx.typeck_results().expr_ty(receiver);
            let peeled_ty = receiver_ty.peel_refs();

            let is_storage_access = if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                let did = adt_def.did();
                match_soroban_def_path(cx, did, &["soroban_sdk", "storage", "Storage"])
                    || match_soroban_def_path(cx, did, &["soroban_sdk", "storage", "Instance"])
                    || match_soroban_def_path(cx, did, &["soroban_sdk", "storage", "Persistent"])
                    || match_soroban_def_path(cx, did, &["soroban_sdk", "storage", "Temporary"])
                    || (match_soroban_def_path(cx, did, &["soroban_sdk", "Env"])
                        && path_segment.ident.name.as_str() == "storage")
            } else {
                false
            };

            if is_storage_access
                && let Some(enclosing_expr) = get_enclosing_loop_or_multi_call_closure(cx, expr)
                && let hir::ExprKind::Loop(..) = enclosing_expr.kind
            {
                span_lint_and_help(
                    cx,
                    SOROBAN_STORAGE_IN_LOOP,
                    expr.span,
                    "storage operation inside a loop",
                    None,
                    "move storage operations out of the loop or accumulate mutations in memory first",
                );
            }
        }
    }
}

rustc_session::declare_lint! {
    pub REDUNDANT_ENV_CLONE,
    Warn,
    "redundant clone on Env object"
}
pub struct RedundantEnvClone;
rustc_session::impl_lint_pass!(RedundantEnvClone => [REDUNDANT_ENV_CLONE]);

impl<'tcx> LateLintPass<'tcx> for RedundantEnvClone {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::MethodCall(path_segment, receiver, _args, _span) = expr.kind
            && path_segment.ident.name.as_str() == "clone"
        {
            let receiver_ty = cx.typeck_results().expr_ty(receiver);
            let peeled_ty = receiver_ty.peel_refs();

            let is_env = if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                match_soroban_def_path(cx, adt_def.did(), &["soroban_sdk", "Env"])
            } else {
                false
            };

            if is_env {
                span_lint_and_help(
                    cx,
                    REDUNDANT_ENV_CLONE,
                    expr.span,
                    "redundant clone on Env object",
                    None,
                    "pass Env by reference or value instead of cloning",
                );
            }
        }
    }
}

rustc_session::declare_lint! {
    pub UNNECESSARY_HOST_FUNCTION_CALL,
    Warn,
    "unnecessary host function call inside loop"
}
pub struct UnnecessaryHostFunctionCall;
rustc_session::impl_lint_pass!(UnnecessaryHostFunctionCall => [UNNECESSARY_HOST_FUNCTION_CALL]);

rustc_session::declare_lint! {
    pub HOST_IN_LOOP,
    Warn,
    "use of Host object inside a loop"
}
pub struct HostInLoop;
rustc_session::impl_lint_pass!(HostInLoop => [HOST_IN_LOOP]);

impl<'tcx> LateLintPass<'tcx> for UnnecessaryHostFunctionCall {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::MethodCall(_path_segment, receiver, _args, _span) = expr.kind {
            let receiver_ty = cx.typeck_results().expr_ty(receiver);
            let peeled_ty = receiver_ty.peel_refs();

            let is_host_function = if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                match_soroban_def_path(cx, adt_def.did(), &["soroban_sdk", "ledger", "Ledger"])
            } else {
                false
            };

            if is_host_function
                && let Some(enclosing_expr) = get_enclosing_loop_or_multi_call_closure(cx, expr)
                && let hir::ExprKind::Loop(..) = enclosing_expr.kind
            {
                span_lint_and_help(
                    cx,
                    UNNECESSARY_HOST_FUNCTION_CALL,
                    expr.span,
                    "unnecessary host function call inside loop",
                    None,
                    "call this function outside the loop and reuse the result",
                );
            }
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for HostInLoop {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::MethodCall(_path_segment, receiver, _args, _span) = expr.kind {
            let receiver_ty = cx.typeck_results().expr_ty(receiver);
            let peeled_ty = receiver_ty.peel_refs();

            let is_host = if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                match_soroban_def_path(cx, adt_def.did(), &["host", "Host"])
            } else {
                false
            };

            if is_host
                && let Some(enclosing_expr) = get_enclosing_loop_or_multi_call_closure(cx, expr)
                && let hir::ExprKind::Loop(..) = enclosing_expr.kind
            {
                span_lint_and_help(
                    cx,
                    HOST_IN_LOOP,
                    expr.span,
                    "use of Host object inside a loop",
                    None,
                    "consider moving the Host usage outside the loop if possible",
                );
            }
        }
    }
}

rustc_session::declare_lint! {
    pub COLLECTION_LEN_IN_LOOP_CONDITION,
    Warn,
    "len() called on Soroban collection inside a loop"
}
pub struct CollectionLenInLoopCondition;
rustc_session::impl_lint_pass!(CollectionLenInLoopCondition => [COLLECTION_LEN_IN_LOOP_CONDITION]);

const MUTATION_METHODS: &[&str] = &[
    "push",
    "pop",
    "insert",
    "set",
    "remove",
    "clear",
    "truncate",
    "swap_remove",
];

impl CollectionLenInLoopCondition {
    fn is_soroban_collection<'tcx>(
        &self,
        cx: &LateContext<'tcx>,
        adt_def: &rustc_middle::ty::AdtDef<'tcx>,
    ) -> bool {
        let did = adt_def.did();
        match_soroban_def_path(cx, did, &["soroban_sdk", "Vec"])
            || match_soroban_def_path(cx, did, &["soroban_sdk", "Map"])
            || match_soroban_def_path(cx, did, &["soroban_sdk", "Set"])
    }

    fn has_mutation_in_loop<'tcx>(
        &self,
        cx: &LateContext<'tcx>,
        receiver: &'tcx hir::Expr<'tcx>,
        body: &'tcx hir::Block<'tcx>,
    ) -> bool {
        if let hir::ExprKind::Path(hir::QPath::Resolved(_, path)) = receiver.kind
            && let hir::def::Res::Local(local_id) = path.res
        {
            struct MutationFinder<'a, 'tcx> {
                cx: &'a LateContext<'tcx>,
                local_id: hir::def_id::LocalDefId,
                found: bool,
            }

            impl<'a, 'tcx> hir::intravisit::Visitor<'tcx> for MutationFinder<'a, 'tcx> {
                fn visit_expr(&mut self, expr: &'tcx hir::Expr<'tcx>) {
                    if self.found {
                        return;
                    }
                    if let hir::ExprKind::MethodCall(path_segment, call_receiver, _, _) = expr.kind
                        && let hir::ExprKind::Path(hir::QPath::Resolved(_, call_path)) =
                            call_receiver.kind
                        && let hir::def::Res::Local(call_local) = call_path.res
                        && call_local == self.local_id
                        && MUTATION_METHODS.contains(&path_segment.ident.name.as_str())
                    {
                        self.found = true;
                        return;
                    }
                    hir::intravisit::walk_expr(self, expr);
                }
            }

            let mut visitor = MutationFinder {
                cx,
                local_id,
                found: false,
            };
            hir::intravisit::walk_block(&mut visitor, body);
            visitor.found
        } else {
            false
        }
    }
}

impl<'tcx> LateLintPass<'tcx> for CollectionLenInLoopCondition {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::MethodCall(path_segment, receiver, _args, _span) = expr.kind
            && path_segment.ident.name.as_str() == "len"
        {
            let receiver_ty = cx.typeck_results().expr_ty(receiver);
            let peeled_ty = receiver_ty.peel_refs();

            let is_collection = if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                self.is_soroban_collection(cx, adt_def)
            } else {
                false
            };

            if is_collection
                && let Some(enclosing_expr) = get_enclosing_loop_or_multi_call_closure(cx, expr)
                && let hir::ExprKind::Loop(loop_block, _, _, _) = enclosing_expr.kind
                && !self.has_mutation_in_loop(cx, receiver, loop_block)
            {
                span_lint_and_help(
                    cx,
                    COLLECTION_LEN_IN_LOOP_CONDITION,
                    expr.span,
                    "len() called on Soroban collection inside a loop",
                    None,
                    "bind the collection length before the loop to avoid repeated metered host calls",
                );
            }
        }
    }
}

#[test]
fn ui() {
    dylint_testing::ui_test(env!("CARGO_PKG_NAME"), "ui");
}
