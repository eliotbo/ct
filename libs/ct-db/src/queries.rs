use ct_core::models::*;
use rusqlite::{params, Connection, Result as SqliteResult, OptionalExtension};
use crate::Result;

pub fn find_symbols_by_name(
    conn: &Connection,
    name: &str,
    kind: Option<&str>,
    vis: Option<&str>,
    status: Option<&str>,
    limit: usize,
) -> Result<Vec<Symbol>> {
    let mut query = String::from(
        "SELECT id, symbol_id, crate_id, file_id, path, name, kind, visibility,
                signature, docs, status, span_start, span_end, def_hash
         FROM symbols WHERE name LIKE ?"
    );
    
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(format!("%{}%", name))];
    
    if let Some(k) = kind {
        query.push_str(" AND kind = ?");
        params.push(Box::new(k.to_string()));
    }
    
    if let Some(v) = vis {
        if v != "all" {
            query.push_str(" AND visibility = ?");
            params.push(Box::new(v.to_string()));
        }
    }
    
    if let Some(s) = status {
        query.push_str(" AND status = ?");
        params.push(Box::new(s.to_string()));
    }
    
    query.push_str(" ORDER BY name, path, span_start LIMIT ?");
    params.push(Box::new(limit as i64));
    
    let mut stmt = conn.prepare(&query)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    
    let symbols = stmt.query_map(&param_refs[..], |row| {
        Ok(Symbol {
            symbol_id: hex::encode(row.get::<_, Vec<u8>>(1)?),
            crate_id: row.get(2)?,
            file_id: row.get(3)?,
            path: row.get(4)?,
            name: row.get(5)?,
            kind: parse_symbol_kind(&row.get::<_, String>(6)?),
            visibility: parse_visibility(&row.get::<_, String>(7)?),
            signature: row.get(8)?,
            docs: row.get(9)?,
            status: parse_status(&row.get::<_, String>(10)?),
            span_start: row.get(11)?,
            span_end: row.get(12)?,
            def_hash: row.get(13)?,
        })
    })?
    .collect::<SqliteResult<Vec<_>>>()?;
    
    Ok(symbols)
}

pub fn find_symbol_by_path(
    conn: &Connection,
    path: &str,
) -> Result<Option<Symbol>> {
    let mut stmt = conn.prepare(
        "SELECT id, symbol_id, crate_id, file_id, path, name, kind, visibility,
                signature, docs, status, span_start, span_end, def_hash
         FROM symbols WHERE path = ?"
    )?;
    
    let symbol = stmt.query_row(params![path], |row| {
        Ok(Symbol {
            symbol_id: hex::encode(row.get::<_, Vec<u8>>(1)?),
            crate_id: row.get(2)?,
            file_id: row.get(3)?,
            path: row.get(4)?,
            name: row.get(5)?,
            kind: parse_symbol_kind(&row.get::<_, String>(6)?),
            visibility: parse_visibility(&row.get::<_, String>(7)?),
            signature: row.get(8)?,
            docs: row.get(9)?,
            status: parse_status(&row.get::<_, String>(10)?),
            span_start: row.get(11)?,
            span_end: row.get(12)?,
            def_hash: row.get(13)?,
        })
    })
    .optional()?;
    
    Ok(symbol)
}

pub fn get_status_counts(
    conn: &Connection,
    vis: Option<&str>,
) -> Result<StatusCounts> {
    let where_clause = match vis {
        Some(v) if v != "all" => format!("WHERE visibility = '{}'", v),
        _ => String::from("WHERE 1=1"),
    };
    
    let total: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM symbols {}", where_clause),
        [],
        |row| row.get(0),
    )?;
    
    let implemented: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM symbols {} AND status = 'implemented'", where_clause),
        [],
        |row| row.get(0),
    )?;
    
    let unimplemented: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM symbols {} AND status = 'unimplemented'", where_clause),
        [],
        |row| row.get(0),
    )?;
    
    let todo: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM symbols {} AND status = 'todo'", where_clause),
        [],
        |row| row.get(0),
    )?;
    
    Ok(StatusCounts {
        total,
        implemented,
        unimplemented,
        todo,
    })
}

pub fn get_status_items(
    conn: &Connection,
    vis: Option<&str>,
    unimplemented: bool,
    todo: bool,
    limit: usize,
) -> Result<Vec<StatusItem>> {
    let mut query = String::from(
        "SELECT path, status, kind FROM symbols WHERE 1=1"
    );
    
    if let Some(v) = vis {
        if v != "all" {
            query.push_str(&format!(" AND visibility = '{}'", v));
        }
    }
    
    if unimplemented && !todo {
        query.push_str(" AND status = 'unimplemented'");
    } else if todo && !unimplemented {
        query.push_str(" AND status = 'todo'");
    } else if unimplemented && todo {
        query.push_str(" AND (status = 'unimplemented' OR status = 'todo')");
    }
    
    query.push_str(&format!(" ORDER BY path LIMIT {}", limit));
    
    let mut stmt = conn.prepare(&query)?;
    let items = stmt.query_map([], |row| {
        Ok(StatusItem {
            path: row.get(0)?,
            status: parse_status(&row.get::<_, String>(1)?),
            kind: parse_symbol_kind(&row.get::<_, String>(2)?),
        })
    })?
    .collect::<SqliteResult<Vec<_>>>()?;
    
    Ok(items)
}

fn parse_symbol_kind(s: &str) -> SymbolKind {
    match s {
        "module" => SymbolKind::Module,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "fn" => SymbolKind::Fn,
        "method" => SymbolKind::Method,
        "field" => SymbolKind::Field,
        "variant" => SymbolKind::Variant,
        "type_alias" => SymbolKind::TypeAlias,
        "const" => SymbolKind::Const,
        "static" => SymbolKind::Static,
        "impl" => SymbolKind::Impl,
        _ => SymbolKind::Module, // Default fallback
    }
}

fn parse_visibility(s: &str) -> Visibility {
    match s {
        "public" => Visibility::Public,
        "private" => Visibility::Private,
        _ => Visibility::Private,
    }
}

fn parse_status(s: &str) -> ImplementationStatus {
    match s {
        "implemented" => ImplementationStatus::Implemented,
        "unimplemented" => ImplementationStatus::Unimplemented,
        "todo" => ImplementationStatus::Todo,
        _ => ImplementationStatus::Implemented,
    }
}