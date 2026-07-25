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
        lint: REDUNDANT_VAL_CONVERSION,
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
        REDUNDANT_VAL_CONVERSION,
    ]);
    lint_store.register_late_pass(|_| Box::new(SorobanStorageInLoop));
    lint_store.register_late_pass(|_| Box::new(RedundantEnvClone));
    lint_store.register_late_pass(|_| Box::new(UnnecessaryHostFunctionCall));
    lint_store.register_late_pass(|_| Box::new(HostInLoop));
    lint_store.register_late_pass(|_| Box::new(RedundantValConversion));
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
    pub REDUNDANT_VAL_CONVERSION,
    Warn,
    "redundant conversion to or from Val"
}
pub struct RedundantValConversion;
rustc_session::impl_lint_pass!(RedundantValConversion => [REDUNDANT_VAL_CONVERSION]);

fn unwrap_borrows<'tcx>(mut expr: &'tcx hir::Expr<'tcx>) -> &'tcx hir::Expr<'tcx> {
    while let hir::ExprKind::AddrOf(_, _, inner) = expr.kind {
        expr = inner;
    }
    expr
}

fn is_same_type_ignoring_result<'tcx>(
    cx: &LateContext<'tcx>,
    source: rustc_middle::ty::Ty<'tcx>,
    dest: rustc_middle::ty::Ty<'tcx>,
) -> bool {
    let source = source.peel_refs();
    let dest = dest.peel_refs();

    if source == dest {
        return true;
    }

    if clippy_utils::ty::is_type_diagnostic_item(cx, dest, rustc_span::sym::Result) {
        if let rustc_middle::ty::Adt(_, args) = dest.kind() {
            if args.type_at(0).peel_refs() == source {
                return true;
            }
        }
    }

    if clippy_utils::ty::is_type_diagnostic_item(cx, source, rustc_span::sym::Result) {
        if let rustc_middle::ty::Adt(_, args) = source.kind() {
            if args.type_at(0).peel_refs() == dest {
                return true;
            }
        }
    }

    false
}

fn get_inner_conversion_source_type<'tcx>(
    cx: &LateContext<'tcx>,
    expr: &'tcx hir::Expr<'tcx>,
) -> Option<rustc_middle::ty::Ty<'tcx>> {
    let expr = unwrap_borrows(expr);
    let typeck = cx.typeck_results();

    if let hir::ExprKind::MethodCall(_path, receiver, _args, _span) = expr.kind {
        if let Some(def_id) = typeck.type_dependent_def_id(expr.hir_id) {
            if match_soroban_def_path(cx, def_id, &["IntoVal", "into_val"])
                || match_soroban_def_path(cx, def_id, &["TryIntoVal", "try_into_val"])
            {
                return Some(typeck.expr_ty(receiver).peel_refs());
            }
        }
    } else if let hir::ExprKind::Call(path_expr, args) = expr.kind {
        if let hir::ExprKind::Path(ref qpath) = path_expr.kind {
            if let Some(def_id) = cx.qpath_res(qpath, path_expr.hir_id).opt_def_id() {
                if match_soroban_def_path(cx, def_id, &["FromVal", "from_val"])
                    || match_soroban_def_path(cx, def_id, &["TryFromVal", "try_from_val"])
                    || match_soroban_def_path(cx, def_id, &["IntoVal", "into_val"])
                    || match_soroban_def_path(cx, def_id, &["TryIntoVal", "try_into_val"])
                {
                    if args.len() >= 2 {
                        return Some(typeck.expr_ty(&args[1]).peel_refs());
                    }
                }
            }
        }
    }
    None
}

impl<'tcx> LateLintPass<'tcx> for RedundantValConversion {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        if expr.span.from_expansion() {
            return;
        }

        let typeck = cx.typeck_results();
        let expr_ty = typeck.expr_ty(expr).peel_refs();

        if let hir::ExprKind::MethodCall(_path, receiver, _args, _span) = expr.kind {
            if let Some(def_id) = typeck.type_dependent_def_id(expr.hir_id) {
                if match_soroban_def_path(cx, def_id, &["IntoVal", "into_val"])
                    || match_soroban_def_path(cx, def_id, &["TryIntoVal", "try_into_val"])
                {
                    let receiver_ty = typeck.expr_ty(receiver).peel_refs();

                    if is_same_type_ignoring_result(cx, receiver_ty, expr_ty) {
                        span_lint_and_help(
                            cx,
                            REDUNDANT_VAL_CONVERSION,
                            expr.span,
                            "redundant conversion to the same type",
                            None,
                            "remove this conversion since the value is already the target type",
                        );
                        return;
                    }

                    if let Some(source_ty) = get_inner_conversion_source_type(cx, receiver) {
                        if is_same_type_ignoring_result(cx, source_ty, expr_ty) {
                            span_lint_and_help(
                                cx,
                                REDUNDANT_VAL_CONVERSION,
                                expr.span,
                                "redundant round-trip conversion",
                                None,
                                "remove these conversions and use the original value directly",
                            );
                            return;
                        }
                    }
                }
            }
        } else if let hir::ExprKind::Call(path_expr, args) = expr.kind {
            if let hir::ExprKind::Path(ref qpath) = path_expr.kind {
                if let Some(def_id) = cx.qpath_res(qpath, path_expr.hir_id).opt_def_id() {
                    if match_soroban_def_path(cx, def_id, &["FromVal", "from_val"])
                        || match_soroban_def_path(cx, def_id, &["TryFromVal", "try_from_val"])
                    {
                        if args.len() >= 2 {
                            let source_arg = &args[1];
                            let source_ty = typeck.expr_ty(source_arg).peel_refs();

                            if is_same_type_ignoring_result(cx, source_ty, expr_ty) {
                                span_lint_and_help(
                                    cx,
                                    REDUNDANT_VAL_CONVERSION,
                                    expr.span,
                                    "redundant conversion to the same type",
                                    None,
                                    "remove this conversion since the value is already the target type",
                                );
                                return;
                            }

                            if let Some(inner_source_ty) =
                                get_inner_conversion_source_type(cx, source_arg)
                            {
                                if is_same_type_ignoring_result(cx, inner_source_ty, expr_ty) {
                                    span_lint_and_help(
                                        cx,
                                        REDUNDANT_VAL_CONVERSION,
                                        expr.span,
                                        "redundant round-trip conversion",
                                        None,
                                        "remove these conversions and use the original value directly",
                                    );
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn ui() {
    dylint_testing::ui_test(env!("CARGO_PKG_NAME"), "ui");
}
