#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_ast;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;

use clippy_utils::diagnostics::{span_lint_and_help, span_lint_and_sugg};
use clippy_utils::get_enclosing_loop_or_multi_call_closure;
use clippy_utils::source::snippet_opt;
use clippy_utils::usage::mutated_variables;
use rustc_ast::LitKind;
use rustc_errors::Applicability;
use rustc_hir as hir;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{HirId, HirIdSet};
use rustc_lint::{LateContext, LateLintPass, LintStore};
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;

dylint_linting::dylint_library!();

fn match_soroban_def_path<'tcx>(cx: &LateContext<'tcx>, def_id: DefId, segments: &[&str]) -> bool {
    let full = cx.tcx.def_path_str(def_id);
    let suffix: String = segments.join("::");
    full.ends_with(&suffix)
}

/// Soroban storage accessor types. Every method call on one of these reaches
/// the host's storage subsystem.
const SOROBAN_STORAGE_TYPES: &[&[&str]] = &[
    &["soroban_sdk", "storage", "Storage"],
    &["soroban_sdk", "storage", "Instance"],
    &["soroban_sdk", "storage", "Persistent"],
    &["soroban_sdk", "storage", "Temporary"],
];

/// Soroban host accessor types reachable from `Env`. A method call on any of
/// them crosses the guest/host boundary and is metered, so repeating it inside
/// a loop with unchanged inputs is wasted CPU budget.
///
/// `soroban_sdk::storage::*` is deliberately absent: storage operations in a
/// loop are reported by [`SOROBAN_STORAGE_IN_LOOP`] instead.
const SOROBAN_HOST_TYPES: &[&[&str]] = &[
    &["soroban_sdk", "ledger", "Ledger"],
    &["soroban_sdk", "crypto", "Crypto"],
    &["soroban_sdk", "crypto", "CryptoHazmat"],
    &["soroban_sdk", "crypto", "bls12_381", "Bls12_381"],
    &["soroban_sdk", "crypto", "bn254", "Bn254"],
    &["soroban_sdk", "prng", "Prng"],
    &["soroban_sdk", "events", "Events"],
    &["soroban_sdk", "deploy", "Deployer"],
    &["soroban_sdk", "deploy", "DeployerWithAddress"],
    &["soroban_sdk", "deploy", "DeployerWithAsset"],
];

/// Host calls that live directly on `Env` rather than on an accessor type, and
/// whose result is constant for the whole invocation.
///
/// The accessor methods themselves (`Env::ledger`, `Env::crypto`, ...) are not
/// listed: they only build a wrapper value, the metered work happens in the
/// method called on the wrapper. Argument-taking `Env` methods such as
/// `invoke_contract` or `authorize_as_current_contract` are also excluded
/// because their cost is inherent to what the loop is doing.
const SOROBAN_ENV_HOST_METHODS: &[&str] = &["current_contract_address"];

/// Soroban SDK container types. Growth-method calls (append, push_back, insert,
/// extend_from_array) on these inside a loop reallocate host-side state on
/// every iteration.
const SOROBAN_CONTAINER_TYPES: &[&[&str]] = &[
    &["soroban_sdk", "Bytes"],
    &["soroban_sdk", "Vec"],
    &["soroban_sdk", "Map"],
];

/// Methods on [`SOROBAN_CONTAINER_TYPES`] that grow the container's backing
/// buffer, causing increasingly expensive host-side work per call.
const BYTES_APPEND_METHODS: &[&str] = &["append", "push_back", "insert", "extend_from_array"];

fn matches_any_path<'tcx>(cx: &LateContext<'tcx>, def_id: DefId, paths: &[&[&str]]) -> bool {
    paths
        .iter()
        .any(|segments| match_soroban_def_path(cx, def_id, segments))
}

fn match_soroban_def_path_tcx(tcx: TyCtxt<'_>, def_id: DefId, segments: &[&str]) -> bool {
    let full = tcx.def_path_str(def_id);
    let suffix: String = segments.join("::");
    full.ends_with(&suffix)
}

fn matches_any_path_tcx(tcx: TyCtxt<'_>, def_id: DefId, paths: &[&[&str]]) -> bool {
    paths
        .iter()
        .any(|segments| match_soroban_def_path_tcx(tcx, def_id, segments))
}

/// Maximum call depth for inter-procedural analysis. Functions reachable
/// beyond this depth are not inspected; the analysis conservatively treats
/// them as not containing the target operation.
const MAX_CALL_DEPTH: u32 = 3;

/// Whether the function identified by `def_id` — or any function it calls
/// up to `depth_remaining` levels deep — performs a Soroban storage or host
/// operation matching `target_paths`.
///
/// # Conservative posture
///
/// External crates (`!def_id.is_local()`), opaque calls (trait methods,
/// function pointers, closures), cycle revisits, and exhausted depth all
/// return `false`. This errs on the side of **not** flagging, so a
/// cross-function lint never produces a false positive from incomplete
/// information.
fn callee_contains_soroban_op<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: DefId,
    target_paths: &[&[&str]],
    depth_remaining: u32,
    visited: &mut Vec<DefId>,
) -> bool {
    // External crate — we can't walk soroban-sdk or std
    if !def_id.is_local() {
        return false;
    }
    // Already visited in this call chain (cycle / recursion)
    if visited.contains(&def_id) {
        return false;
    }
    // Depth bound exhausted
    if depth_remaining == 0 {
        return false;
    }

    visited.push(def_id);

    let local_def_id = def_id.expect_local();
    let body_id = tcx
        .hir_node_by_def_id(local_def_id)
        .body_id()
        .expect("callee has no body");
    let body = tcx.hir_body(body_id);
    let typeck = tcx.typeck(local_def_id);

    let found = {
        let mut detector = CalleeStorageDetector {
            tcx,
            typeck,
            target_paths,
            depth_remaining: depth_remaining - 1,
            visited,
            found: false,
        };
        detector.visit_expr(body.value);
        detector.found
    };

    visited.pop();
    found
}

/// Visitor that walks a callee body looking for direct storage/host method
/// calls or nested calls that transitively reach one.
struct CalleeStorageDetector<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typeck: &'tcx rustc_middle::ty::TypeckResults<'tcx>,
    target_paths: &'a [&'a [&'a str]],
    depth_remaining: u32,
    visited: &'a mut Vec<DefId>,
    found: bool,
}

impl<'a, 'tcx> Visitor<'tcx> for CalleeStorageDetector<'a, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx hir::Expr<'tcx>) {
        if self.found {
            return;
        }

        match expr.kind {
            hir::ExprKind::MethodCall(path_segment, receiver, _args, _span) => {
                let receiver_ty = self.typeck.expr_ty(receiver);
                let peeled_ty = receiver_ty.peel_refs();

                if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                    let did = adt_def.did();
                    if matches_any_path_tcx(self.tcx, did, self.target_paths)
                        || (match_soroban_def_path_tcx(self.tcx, did, &["soroban_sdk", "Env"])
                            && path_segment.ident.name.as_str() == "storage")
                    {
                        self.found = true;
                        return;
                    }
                }
            }
            hir::ExprKind::Call(_callee, _args) => {
                // Check whether this nested call transitively reaches a
                // storage/host op.
                if let Some(callee_def_id) = self.typeck.type_dependent_def_id(expr.hir_id)
                    && callee_contains_soroban_op(
                        self.tcx,
                        callee_def_id,
                        self.target_paths,
                        self.depth_remaining,
                        self.visited,
                    )
                {
                    self.found = true;
                    return;
                }
            }
            _ => {}
        }

        intravisit::walk_expr(self, expr);
    }
}

/// Collects the `HirId`s of every binding introduced inside the visited
/// subtree, e.g. the loop variable of a `for` loop or a per-iteration `let`.
#[derive(Default)]
struct BindingCollector {
    bindings: HirIdSet,
}

impl<'tcx> Visitor<'tcx> for BindingCollector {
    fn visit_pat(&mut self, pat: &'tcx hir::Pat<'tcx>) {
        if let hir::PatKind::Binding(_, hir_id, _, _) = pat.kind {
            self.bindings.insert(hir_id);
        }
        intravisit::walk_pat(self, pat);
    }
}

/// Collects the `HirId`s of every local read in the visited subtree.
#[derive(Default)]
struct LocalReadCollector {
    reads: Vec<HirId>,
}

impl<'tcx> Visitor<'tcx> for LocalReadCollector {
    fn visit_expr(&mut self, expr: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::Path(hir::QPath::Resolved(None, path)) = expr.kind
            && let hir::def::Res::Local(hir_id) = path.res
        {
            self.reads.push(hir_id);
        }
        intravisit::walk_expr(self, expr);
    }
}

/// Whether `call` — receiver chain and arguments included — reads anything that
/// changes from iteration to iteration of `loop_expr`.
///
/// Such a call is doing real per-iteration work, so hoisting it out of the loop
/// would change behaviour and it must not be reported. The answer errs towards
/// "depends": when the mutation analysis cannot give a verdict, the call is
/// treated as loop-dependent and stays unreported.
///
/// Known gaps, all of which cause a call to be reported rather than skipped:
/// bindings and mutations inside a closure body nested in the loop are not
/// seen, and mutation through a raw pointer or interior mutability (`RefCell`,
/// `Cell`) is not tracked.
fn depends_on_loop_state<'tcx>(
    cx: &LateContext<'tcx>,
    loop_expr: &'tcx hir::Expr<'tcx>,
    call: &'tcx hir::Expr<'tcx>,
) -> bool {
    let Some(mutated) = mutated_variables(loop_expr, cx) else {
        return true;
    };

    let mut bound = BindingCollector::default();
    bound.visit_expr(loop_expr);

    let mut read = LocalReadCollector::default();
    read.visit_expr(call);

    read.reads
        .iter()
        .any(|hir_id| bound.bindings.contains(hir_id) || mutated.contains(hir_id))
}

/// Whether `expr` sits directly inside a loop body, returning that loop.
///
/// A call inside a closure that the loop calls is not reported: the closure may
/// well be defined elsewhere, and the receiver is out of reach for the
/// loop-dependence analysis.
fn enclosing_loop<'tcx>(
    cx: &LateContext<'tcx>,
    expr: &'tcx hir::Expr<'tcx>,
) -> Option<&'tcx hir::Expr<'tcx>> {
    let enclosing = get_enclosing_loop_or_multi_call_closure(cx, expr)?;
    matches!(enclosing.kind, hir::ExprKind::Loop(..)).then_some(enclosing)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintCategory {
    StorageOperations,
    Compute,
    Memory,
    EntryLifecycle,
    SymbolOperations,
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
        lint: SYMBOL_NEW_FOR_SHORT_LITERAL,
        category: LintCategory::SymbolOperations,
    },
    LintMetadata {
        lint: BYTES_APPEND_IN_LOOP,
        category: LintCategory::Memory,
    },
];

#[unsafe(no_mangle)]
pub fn register_lints(_sess: &rustc_session::Session, lint_store: &mut LintStore) {
    lint_store.register_lints(&[
        SOROBAN_STORAGE_IN_LOOP,
        REDUNDANT_ENV_CLONE,
        UNNECESSARY_HOST_FUNCTION_CALL,
        HOST_IN_LOOP,
        SYMBOL_NEW_FOR_SHORT_LITERAL,
        BYTES_APPEND_IN_LOOP,
    ]);
    lint_store.register_late_pass(|_| Box::new(SorobanStorageInLoop));
    lint_store.register_late_pass(|_| Box::new(RedundantEnvClone));
    lint_store.register_late_pass(|_| Box::new(UnnecessaryHostFunctionCall));
    lint_store.register_late_pass(|_| Box::new(HostInLoop));
    lint_store.register_late_pass(|_| Box::new(SymbolNewForShortLiteral));
    lint_store.register_late_pass(|_| Box::new(BytesAppendInLoop));
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
        // --- Direct storage access in a loop ---
        if let hir::ExprKind::MethodCall(path_segment, receiver, _args, _span) = expr.kind {
            let receiver_ty = cx.typeck_results().expr_ty(receiver);
            let peeled_ty = receiver_ty.peel_refs();

            let is_storage_access = if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                let did = adt_def.did();
                matches_any_path(cx, did, SOROBAN_STORAGE_TYPES)
                    || (match_soroban_def_path(cx, did, &["soroban_sdk", "Env"])
                        && path_segment.ident.name.as_str() == "storage")
            } else {
                false
            };

            if is_storage_access && enclosing_loop(cx, expr).is_some() {
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

        // --- Inter-procedural: call that transitively reaches storage ---
        if let hir::ExprKind::Call(_callee, _args) = expr.kind
            && enclosing_loop(cx, expr).is_some()
        {
            let mut visited: Vec<DefId> = Vec::new();
            if let Some(callee_def_id) = cx.typeck_results().type_dependent_def_id(expr.hir_id)
                && callee_contains_soroban_op(
                    cx.tcx,
                    callee_def_id,
                    SOROBAN_STORAGE_TYPES,
                    MAX_CALL_DEPTH,
                    &mut visited,
                )
            {
                span_lint_and_help(
                    cx,
                    SOROBAN_STORAGE_IN_LOOP,
                    expr.span,
                    "storage operation inside a loop (reached through function call)",
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
        if let hir::ExprKind::MethodCall(path_segment, receiver, _args, _span) = expr.kind {
            let receiver_ty = cx.typeck_results().expr_ty(receiver);
            let peeled_ty = receiver_ty.peel_refs();

            let is_host_function = if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                let did = adt_def.did();
                matches_any_path(cx, did, SOROBAN_HOST_TYPES)
                    || (match_soroban_def_path(cx, did, &["soroban_sdk", "Env"])
                        && SOROBAN_ENV_HOST_METHODS.contains(&path_segment.ident.name.as_str()))
            } else {
                false
            };

            if is_host_function
                && let Some(loop_expr) = enclosing_loop(cx, expr)
                && !depends_on_loop_state(cx, loop_expr, expr)
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

            if is_host && enclosing_loop(cx, expr).is_some() {
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

// =======================================================================
// symbol_new_for_short_literal — Lint
// =======================================================================

rustc_session::declare_lint! {
    pub SYMBOL_NEW_FOR_SHORT_LITERAL,
    Warn,
    "Symbol::new used with a short literal that could use symbol_short! macro"
}
pub struct SymbolNewForShortLiteral;
rustc_session::impl_lint_pass!(SymbolNewForShortLiteral => [SYMBOL_NEW_FOR_SHORT_LITERAL]);

impl<'tcx> LateLintPass<'tcx> for SymbolNewForShortLiteral {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        // Check for Symbol::new(&env, "literal") calls
        if let hir::ExprKind::Call(callee, args) = expr.kind
            && args.len() == 2
            && let hir::ExprKind::Path(ref qpath) = callee.kind
            && let Some(def_id) = cx.qpath_res(qpath, callee.hir_id).opt_def_id()
            && match_soroban_def_path(cx, def_id, &["soroban_sdk", "Symbol", "new"])
        {
            // Check if the second argument is a string literal
            if let hir::ExprKind::Lit(lit) = args[1].kind
                && let LitKind::Str(symbol, _) = lit.node
            {
                let s = symbol.as_str();
                if is_valid_short_symbol(s) {
                    // Check if there's a valid suggestion
                    if let Some(snippet) = snippet_opt(cx, args[1].span) {
                        let suggestion = format!("symbol_short!({})", snippet);
                        span_lint_and_sugg(
                            cx,
                            SYMBOL_NEW_FOR_SHORT_LITERAL,
                            expr.span,
                            "Symbol::new called with a short literal that could use symbol_short! macro",
                            "use symbol_short! macro for compile-time symbol creation",
                            suggestion,
                            Applicability::MachineApplicable,
                        );
                    } else {
                        span_lint_and_help(
                            cx,
                            SYMBOL_NEW_FOR_SHORT_LITERAL,
                            expr.span,
                            "Symbol::new called with a short literal that could use symbol_short! macro",
                            None,
                            "use symbol_short! macro for compile-time symbol creation",
                        );
                    }
                }
            }
        }
    }
}

// =======================================================================
// bytes_append_in_loop — Lint
// =======================================================================

rustc_session::declare_lint! {
    pub BYTES_APPEND_IN_LOOP,
    Warn,
    "repeatedly growing SDK containers inside loops"
}
pub struct BytesAppendInLoop;
rustc_session::impl_lint_pass!(BytesAppendInLoop => [BYTES_APPEND_IN_LOOP]);

impl<'tcx> LateLintPass<'tcx> for BytesAppendInLoop {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::MethodCall(path_segment, receiver, _args, _span) = expr.kind {
            let method_name = path_segment.ident.name.as_str();
            if !BYTES_APPEND_METHODS.contains(&method_name) {
                return;
            }

            let receiver_ty = cx.typeck_results().expr_ty(receiver);
            let peeled_ty = receiver_ty.peel_refs();

            let is_container = if let rustc_middle::ty::Adt(adt_def, _) = peeled_ty.kind() {
                matches_any_path(cx, adt_def.did(), SOROBAN_CONTAINER_TYPES)
            } else {
                false
            };

            if is_container && enclosing_loop(cx, expr).is_some() {
                span_lint_and_help(
                    cx,
                    BYTES_APPEND_IN_LOOP,
                    expr.span,
                    "repeatedly growing SDK container inside a loop",
                    None,
                    "accumulate values in native Rust collections first, then batch host \
                     operations or convert to SDK containers once after the loop; \
                     pre-size where practical",
                );
            }
        }
    }
}

/// Check if a string is a valid short symbol (<= 9 chars, only a-zA-Z0-9_)
fn is_valid_short_symbol(s: &str) -> bool {
    if s.len() > 9 || s.is_empty() {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[test]
fn ui() {
    dylint_testing::ui_test(env!("CARGO_PKG_NAME"), "ui");
}
