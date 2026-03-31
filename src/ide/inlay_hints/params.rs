use cairo_lang_filesystem::ids::FileId;
use cairo_lang_semantic::Expr;
use cairo_lang_semantic::db::SemanticGroup;
use cairo_lang_semantic::items::function_with_body::{FunctionWithBodySemantic, SemanticExprLookup};
use cairo_lang_semantic::items::functions::FunctionsSemantic;
use cairo_lang_semantic::lookup_item::LookupItemEx;
use cairo_lang_syntax::node::ast::{ArgClause, BinaryOperator, ExprBinary, ExprFunctionCall};
use cairo_lang_syntax::node::{SyntaxNode, TypedSyntaxNode};
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel};

use crate::lang::db::{AnalysisDatabase, LsSemanticGroup};
use crate::lang::lsp::ToLsp;
use crate::lang::proc_macros::db::get_og_node;

pub fn param_inlay_hints<'db>(
    db: &'db AnalysisDatabase,
    file: FileId<'db>,
    call_syntax: ExprFunctionCall<'db>,
) -> Vec<InlayHint> {
    param_inlay_hints_inner(db, file, call_syntax).unwrap_or_default()
}

fn param_inlay_hints_inner<'db>(
    db: &'db AnalysisDatabase,
    file: FileId<'db>,
    call_syntax: ExprFunctionCall<'db>,
) -> Option<Vec<InlayHint>> {
    let call_node = call_syntax.as_syntax_node();

    let is_method_call = call_node
        .parent(db)
        .and_then(|parent| ExprBinary::cast(db, parent))
        .is_some_and(|binary| matches!(binary.op(db), BinaryOperator::Dot(_)));

    let resultants = db.get_node_resultants(call_node)?;

    for resultant in resultants {
        let Some(resultant_call) = ExprFunctionCall::cast(db, *resultant) else {
            continue;
        };

        let lookup_item = db.find_lookup_item(resultant_call.as_syntax_node())?;
        let function_with_body = lookup_item.function_with_body()?;

        let semantic_expr = if is_method_call {
            let parent = resultant_call.as_syntax_node().parent(db)?;
            let binary = ExprBinary::cast(db, parent)?;
            let expr_id =
                db.lookup_expr_by_ptr(function_with_body, binary.stable_ptr(db).into()).ok()?;
            let semantic_db: &dyn SemanticGroup = db;
            semantic_db.expr_semantic(function_with_body, expr_id)
        } else {
            let expr_id = db
                .lookup_expr_by_ptr(function_with_body, resultant_call.stable_ptr(db).into())
                .ok()?;
            let semantic_db: &dyn SemanticGroup = db;
            semantic_db.expr_semantic(function_with_body, expr_id)
        };

        let Expr::FunctionCall(func_call) = semantic_expr else {
            continue;
        };

        let signature = db.concrete_function_signature(func_call.function).ok()?;

        let syntax_args: Vec<_> =
            call_syntax.arguments(db).arguments(db).elements(db).collect();

        let params_to_zip: Vec<_> = signature
            .params
            .iter()
            .filter(|p| p.name.to_string(db) != "self")
            .collect();

        if syntax_args.len() != params_to_zip.len() {
            return None;
        }

        let mut hints = Vec::new();

        for (arg, param) in syntax_args.iter().zip(params_to_zip.iter()) {
            let arg_clause = arg.arg_clause(db);
            match &arg_clause {
                ArgClause::Named(_) | ArgClause::FieldInitShorthand(_) => continue,
                ArgClause::Unnamed(unnamed) => {
                    let param_name = param.name.to_string(db);

                    if should_skip_hint(db, &unnamed.value(db), &param_name) {
                        continue;
                    }

                    let Some(og_arg_node) = get_og_node(db, arg.as_syntax_node()) else {
                        continue;
                    };

                    hints.extend(param_name_inlay_hint(db, file, og_arg_node, &param_name));
                }
            }
        }

        return Some(hints);
    }

    None
}

fn should_skip_hint(
    db: &AnalysisDatabase,
    arg_expr: &cairo_lang_syntax::node::ast::Expr,
    param_name: &str,
) -> bool {
    if let cairo_lang_syntax::node::ast::Expr::Path(path) = arg_expr {
        let text = path.as_syntax_node().get_text_without_trivia(db).to_string(db);
        if text == param_name {
            return true;
        }
    }

    matches!(arg_expr, cairo_lang_syntax::node::ast::Expr::Closure(_))
}

fn param_name_inlay_hint<'db>(
    db: &'db AnalysisDatabase,
    file: FileId<'db>,
    node: SyntaxNode<'db>,
    param_name: &str,
) -> Option<InlayHint> {
    Some(InlayHint {
        position: node.span_without_trivia(db).position_in_file(db, file)?.start.to_lsp(),
        label: InlayHintLabel::String(format!("{param_name}: ")),
        kind: Some(InlayHintKind::PARAMETER),
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right: None,
        data: None,
    })
}
