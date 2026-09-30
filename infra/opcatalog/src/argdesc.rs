use crate::argdesc_data::{COMMON_ARG_DESC, PER_OP_ARG_DESC};

fn lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

pub(crate) fn arg_description(kind: &str, name: &str, typ: &str) -> String {
    if let Some(d) = lookup(PER_OP_ARG_DESC, &format!("{kind}.{name}")) {
        return d.to_string();
    }
    if let Some(d) = lookup(COMMON_ARG_DESC, name) {
        return d.to_string();
    }
    match typ {
        "tail" => "Remaining tokens on the line (parsed at load time).".into(),
        "word" => "Bare word token.".into(),
        "ident" => "Identifier binding or command name.".into(),
        "string" => format!("{name} (string)."),
        "int" => format!("{name} (integer)."),
        "any" => format!("{name} (any value)."),
        _ => name.to_string(),
    }
}
