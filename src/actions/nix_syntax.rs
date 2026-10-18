//! Static attribute discovery without evaluating user code or interpreting strings as Nix.
use anyhow::{Result, bail};
use rnix::ast::{self, HasEntry};
use rowan::ast::AstNode;

fn literal_attribute(attr: ast::Attr) -> Option<String> {
    match attr {
        ast::Attr::Ident(ident) => Some(ident.to_string()),
        ast::Attr::Str(value) => {
            let mut name = String::new();
            for part in value.normalized_parts() {
                match part {
                    ast::InterpolPart::Literal(text) => name.push_str(&text),
                    ast::InterpolPart::Interpolation(_) => return None,
                }
            }
            Some(name)
        }
        ast::Attr::Dynamic(_) => None,
    }
}

fn insert_name(names: &mut Vec<String>, name: Option<String>) {
    if let Some(name) = name.filter(|name| !name.is_empty())
        && !names.contains(&name)
    {
        names.push(name);
    }
}

/// Return statically declared names under an attribute such as inputs or nixosConfigurations.
/// Dynamic/imported names remain undiscovered; no Nix expression is evaluated here.
pub fn attribute_names(content: &str, scope: &str) -> Result<Vec<String>> {
    let parsed = rnix::Root::parse(content);
    if !parsed.errors().is_empty() {
        bail!("Invalid Nix syntax: {:?}", parsed.errors());
    }
    let mut names = Vec::new();
    for entry in parsed
        .syntax()
        .descendants()
        .filter_map(ast::AttrpathValue::cast)
    {
        let Some(path) = entry.attrpath() else {
            continue;
        };
        let mut attrs = path.attrs();
        if attrs.next().and_then(literal_attribute).as_deref() != Some(scope) {
            continue;
        }
        if let Some(name) = attrs.next() {
            insert_name(&mut names, literal_attribute(name));
            continue;
        }
        // Traverse only structural wrappers of this value, not arbitrary nested data.
        let mut pending: Vec<ast::Expr> = entry.value().into_iter().collect();
        while let Some(value) = pending.pop() {
            match value {
                ast::Expr::AttrSet(set) => {
                    for value in set.attrpath_values() {
                        insert_name(
                            &mut names,
                            value
                                .attrpath()
                                .and_then(|path| path.attrs().next())
                                .and_then(literal_attribute),
                        );
                    }
                    for inherit in set.inherits() {
                        for name in inherit.attrs() {
                            insert_name(&mut names, literal_attribute(name));
                        }
                    }
                }
                ast::Expr::Paren(paren) => pending.extend(paren.expr()),
                ast::Expr::BinOp(op) if op.operator() == Some(ast::BinOpKind::Update) => {
                    pending.extend(op.rhs());
                    pending.extend(op.lhs());
                }
                _ => {}
            }
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_comments_and_merge_do_not_hide_hosts() {
        let content = r#"{ outputs = _: {
            nixosConfigurations = {} // {
                first = build { message = "}"; multiline = '' { } ''; };
                /* } nixosConfigurations.fake = {}; */
                "second" = build {};
            };
        }; }"#;
        assert_eq!(
            attribute_names(content, "nixosConfigurations").unwrap(),
            ["first", "second"]
        );
    }

    #[test]
    fn inputs_only_collect_direct_children() {
        let content =
            r#"{ inputs = { nixpkgs.url = "x"; hm = { inputs.nixpkgs.follows = "nixpkgs"; }; }; }"#;
        assert_eq!(
            attribute_names(content, "inputs").unwrap(),
            ["nixpkgs", "hm"]
        );
    }

    #[test]
    fn invalid_nix_is_an_error() {
        assert!(attribute_names("{ inputs = { broken =", "inputs").is_err());
    }
}
