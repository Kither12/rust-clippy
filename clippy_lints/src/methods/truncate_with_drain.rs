use clippy_utils::consts::{ConstEvalCtxt, mir_to_const};
use clippy_utils::diagnostics::span_lint_and_sugg;
use clippy_utils::higher;
use clippy_utils::source::snippet;
use clippy_utils::ty::{is_type_diagnostic_item, is_type_lang_item};
use rustc_ast::ast::RangeLimits;
use rustc_errors::Applicability;
use rustc_hir::{Expr, ExprKind, LangItem, Path, QPath};
use rustc_lint::LateContext;
use rustc_middle::mir::Const;
use rustc_middle::ty::{self as rustc_ty};
use rustc_span::Span;
use rustc_span::symbol::sym;

use super::TRUNCATE_WITH_DRAIN;

// Add `String` here when it is added to diagnostic items
const ACCEPTABLE_TYPES_WITH_ARG: [rustc_span::Symbol; 2] = [sym::Vec, sym::VecDeque];

pub fn is_range_open_ended(cx: &LateContext<'_>, expr: &Expr<'_>, container_path: Option<&Path<'_>>) -> bool {
    let ty = cx.typeck_results().expr_ty(expr);
    if let Some(higher::Range { start, end, limits }) = higher::Range::hir(expr) {
        let start_is_none_or_min = start.map_or(true, |start| {
            if let rustc_ty::Adt(_, subst) = ty.kind()
                && let bnd_ty = subst.type_at(0)
                && let Some(min_val) = bnd_ty.numeric_min_val(cx.tcx)
                && let Some(min_const) = mir_to_const(cx.tcx, Const::from_ty_const(min_val, bnd_ty, cx.tcx))
                && let Some(start_const) = ConstEvalCtxt::new(cx).eval(start)
            {
                start_const == min_const
            } else {
                false
            }
        });
        let end_is_none_or_max = end.map_or(true, |end| match limits {
            RangeLimits::Closed => {
                if let rustc_ty::Adt(_, subst) = ty.kind()
                    && let bnd_ty = subst.type_at(0)
                    && let Some(max_val) = bnd_ty.numeric_max_val(cx.tcx)
                    && let Some(max_const) = mir_to_const(cx.tcx, Const::from_ty_const(max_val, bnd_ty, cx.tcx))
                    && let Some(end_const) = ConstEvalCtxt::new(cx).eval(end)
                {
                    end_const == max_const
                } else {
                    false
                }
            },
            RangeLimits::HalfOpen => {
                if let Some(container_path) = container_path
                    && let ExprKind::MethodCall(name, self_arg, [], _) = end.kind
                    && name.ident.name == sym::len
                    && let ExprKind::Path(QPath::Resolved(None, path)) = self_arg.kind
                {
                    container_path.res == path.res
                } else {
                    false
                }
            },
        });
        return !start_is_none_or_min && end_is_none_or_max;
    }
    false
}

pub(super) fn check(cx: &LateContext<'_>, expr: &Expr<'_>, recv: &Expr<'_>, span: Span, arg: Option<&Expr<'_>>) {
    if let Some(arg) = arg {
        if match_acceptable_type(cx, recv, &ACCEPTABLE_TYPES_WITH_ARG)
            && let ExprKind::Path(QPath::Resolved(None, container_path)) = recv.kind
            && is_range_open_ended(cx, arg, Some(container_path))
        {
            suggest(cx, expr, recv, span, arg);
        }
    }
}

fn match_acceptable_type(cx: &LateContext<'_>, expr: &Expr<'_>, types: &[rustc_span::Symbol]) -> bool {
    let expr_ty = cx.typeck_results().expr_ty(expr).peel_refs();
    types.iter().any(|&ty| is_type_diagnostic_item(cx, expr_ty, ty))
    // String type is a lang item but not a diagnostic item for now so we need a separate check
        || is_type_lang_item(cx, expr_ty, LangItem::String)
}

fn suggest(cx: &LateContext<'_>, expr: &Expr<'_>, recv: &Expr<'_>, span: Span, arg: &Expr<'_>) {
    if let Some(adt) = cx.typeck_results().expr_ty(recv).ty_adt_def()
    // Use `opt_item_name` while `String` is not a diagnostic item
        && let Some(ty_name) = cx.tcx.opt_item_name(adt.did())
    {
        if let Some(higher::Range { start: Some(start), .. }) = higher::Range::hir(arg) {
            span_lint_and_sugg(
                cx,
                TRUNCATE_WITH_DRAIN,
                span.with_hi(expr.span.hi()),
                format!("`drain` used to truncate a `{ty_name}`"),
                "try",
                format!("truncate({})", snippet(cx, start.span, "0")),
                Applicability::MachineApplicable,
            );
        }
    }
}
