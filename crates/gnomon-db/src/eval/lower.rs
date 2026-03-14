use std::path::PathBuf;

use gnomon_parser::ast;
use gnomon_parser::{SyntaxKind, SyntaxToken};

use super::desugar;
use super::interned::{DeclId, DeclKind, FieldName, FieldPath};
use super::literals;
use super::types::{Blame, Blamed, Record, Value};
use crate::input::SourceFile;
use crate::queries::Diagnostic;

/// Evaluate a string or triple-string token to its string value.
fn eval_string_token(token: &SyntaxToken) -> String {
    match token.kind() {
        SyntaxKind::TRIPLE_STRING_LITERAL => literals::eval_triple_string(token.text()),
        _ => literals::eval_string(token.text()),
    }
}

#[derive(Debug, Clone, Copy)]
enum ImportFormat {
    Gnomon,
    ICalendar,
    JSCalendar,
    Infer,
}

fn infer_format_from_content_type(ct: &str) -> ImportFormat {
    if ct.starts_with("text/calendar") {
        ImportFormat::ICalendar
    } else if ct.starts_with("application/json") || ct.starts_with("application/jscalendar+json") {
        ImportFormat::JSCalendar
    } else {
        ImportFormat::Gnomon
    }
}

pub struct LowerCtx<'db> {
    db: &'db dyn crate::Db,
    source: SourceFile,
    pub diagnostics: Vec<Diagnostic>,
    /// Environment for let bindings (name, value). Scanned back-to-front for shadowing.
    env: Vec<(String, Value<'db>)>,
    /// Stack of file paths currently being evaluated, for cycle detection.
    import_stack: Vec<PathBuf>,
    /// All transitively imported Gnomon file paths (canonical).
    pub imported_files: Vec<PathBuf>,
    /// If true, bypass the URI import cache and always re-fetch.
    pub force_refresh: bool,
}

impl<'db> LowerCtx<'db> {
    pub fn new(db: &'db dyn crate::Db, source: SourceFile) -> Self {
        let path = source.path(db).clone();
        Self {
            db,
            source,
            diagnostics: Vec::new(),
            env: Vec::new(),
            import_stack: vec![path],
            imported_files: Vec::new(),
            force_refresh: false,
        }
    }

    pub(super) fn with_import_stack(
        db: &'db dyn crate::Db,
        source: SourceFile,
        import_stack: Vec<PathBuf>,
    ) -> Self {
        Self {
            db,
            source,
            diagnostics: Vec::new(),
            env: Vec::new(),
            import_stack,
            imported_files: Vec::new(),
            force_refresh: false,
        }
    }

    /// Seed the environment with bindings from prior REPL inputs.
    pub fn seed_env(&mut self, bindings: &[(String, Value<'db>)]) {
        self.env.extend(bindings.iter().cloned());
    }

    /// Return the current number of environment bindings.
    pub fn env_len(&self) -> usize {
        self.env.len()
    }

    /// Extract bindings added since `start_len`.
    pub fn env_slice_from(&self, start_len: usize) -> Vec<(String, Value<'db>)> {
        self.env[start_len..].to_vec()
    }

    /// Lower a source file into a Value.
    ///
    /// File structure: optional `let` bindings, then either declarations or an expression body.
    // r[impl syntax.file.let]
    // r[impl syntax.file.body+2]
    pub fn lower_source_file(&mut self, file: &ast::SourceFile) -> Value<'db> {
        // 1. Process file-level let bindings (pushed into env).
        for binding in file.let_bindings() {
            if let Some(name_tok) = binding.name() {
                let name = name_tok.text().to_string();
                let decl_id = self.make_decl_id(0, DeclKind::Expr);
                let value = match binding.value_expr() {
                    Some(expr) => self.lower_top_expr(&expr, decl_id, &FieldPath::root(self.db)),
                    None => Value::Undefined,
                };
                self.env.push((name, value));
            }
        }

        // 2. Evaluate body expressions
        let exprs: Vec<_> = file.body_exprs().collect();
        if exprs.is_empty() {
            // Empty body → empty list
            Value::List(vec![])
        } else {
            let first_is_decl = matches!(
                &exprs[0],
                ast::Expr::CalendarExpr(_) | ast::Expr::EventExpr(_) | ast::Expr::TaskExpr(_)
            );
            if first_is_decl {
                // List mode: all expressions collected into a list
                let items = exprs
                    .iter()
                    .enumerate()
                    .map(|(i, expr)| {
                        let decl_id = self.make_decl_id(i, decl_kind_for_expr(expr));
                        let value = self.lower_top_expr(expr, decl_id, &FieldPath::root(self.db));
                        Blamed {
                            value,
                            blame: self.root_blame(decl_id),
                        }
                    })
                    .collect();
                Value::List(items)
            } else {
                // Single expression mode
                let decl_id = self.make_decl_id(0, DeclKind::Expr);
                self.lower_top_expr(&exprs[0], decl_id, &FieldPath::root(self.db))
            }
        }
    }

    fn lower_event(&mut self, ev: &ast::EventExpr, decl_id: DeclId<'db>) -> Record<'db> {
        let base_path = FieldPath::root(self.db);

        if ev.name().is_some() {
            // r[impl decl.short-event.desugar+2]
            // Short form: event @name datetime [duration] ["title"] [{ body }]
            let mut record = Record::new();

            if let Some(name_token) = ev.name() {
                let name_str = literals::eval_name(name_token.text());
                self.insert_field(
                    &mut record,
                    "name",
                    Value::Name(name_str),
                    decl_id,
                    &base_path,
                );
            }

            if let Some(span) = ev.short_span() {
                if let Some(dt) = span.start()
                    && let Some(value) = self.lower_short_dt(&dt, decl_id, &base_path, "start")
                {
                    self.insert_field(&mut record, "start", value, decl_id, &base_path);
                }
                if let Some(dur_token) = span.duration() {
                    let blame = self.make_blame(decl_id, &base_path.field(self.db,self.intern("duration")));
                    if let Some(value) =
                        desugar::desugar_duration(self.db, dur_token.text(), &blame)
                    {
                        self.insert_field(&mut record, "duration", value, decl_id, &base_path);
                    }
                }
            }

            if let Some(title_token) = ev.title() {
                let title = eval_string_token(&title_token);
                self.insert_field(
                    &mut record,
                    "title",
                    Value::String(title),
                    decl_id,
                    &base_path,
                );
            }

            if let Some(body) = ev.body() {
                let body_record = self.lower_record(&body, decl_id, &base_path);
                for (name, value) in body_record.into_iter() {
                    record.insert(self.db, name, value);
                }
            }

            record
        } else {
            match ev.body() {
                Some(body) => self.lower_record(&body, decl_id, &base_path),
                None => Record::new(),
            }
        }
    }

    fn lower_task(&mut self, task: &ast::TaskExpr, decl_id: DeclId<'db>) -> Record<'db> {
        let base_path = FieldPath::root(self.db);

        if task.name().is_some() {
            // r[impl decl.short-task.desugar+2]
            let mut record = Record::new();

            if let Some(name_token) = task.name() {
                let name_str = literals::eval_name(name_token.text());
                self.insert_field(
                    &mut record,
                    "name",
                    Value::Name(name_str),
                    decl_id,
                    &base_path,
                );
            }

            if let Some(dt) = task.short_dt()
                && let Some(value) = self.lower_short_dt(&dt, decl_id, &base_path, "due")
            {
                self.insert_field(&mut record, "due", value, decl_id, &base_path);
            }

            if let Some(title_token) = task.title() {
                let title = eval_string_token(&title_token);
                self.insert_field(
                    &mut record,
                    "title",
                    Value::String(title),
                    decl_id,
                    &base_path,
                );
            }

            if let Some(body) = task.body() {
                let body_record = self.lower_record(&body, decl_id, &base_path);
                for (name, value) in body_record.into_iter() {
                    record.insert(self.db, name, value);
                }
            }

            record
        } else {
            match task.body() {
                Some(body) => self.lower_record(&body, decl_id, &base_path),
                None => Record::new(),
            }
        }
    }

    // r[impl expr.record.syntax]
    fn lower_record(
        &mut self,
        record: &ast::RecordExpr,
        decl_id: DeclId<'db>,
        base_path: &FieldPath<'db>,
    ) -> Record<'db> {
        let mut result = Record::new();
        for field in record.fields() {
            let Some(name_token) = field.name() else {
                continue;
            };
            let Some(value_expr) = field.value() else {
                continue;
            };

            let field_name = self.intern(name_token.text());
            let field_path = base_path.field(self.db,field_name);
            let blame = self.make_blame(decl_id, &field_path);

            let value = self.lower_top_expr(&value_expr, decl_id, &field_path);
            result.insert(self.db, field_name, Blamed { value, blame });
        }
        result
    }

    /// Lower a top-level expression (unified dispatch for all expression forms).
    fn lower_top_expr(
        &mut self,
        expr: &ast::Expr,
        decl_id: DeclId<'db>,
        path: &FieldPath<'db>,
    ) -> Value<'db> {
        match expr {
            ast::Expr::LiteralExpr(lit) => self
                .lower_literal(lit, decl_id, path)
                .unwrap_or(Value::Undefined),
            ast::Expr::RecordExpr(rec) => Value::Record(self.lower_record(rec, decl_id, path)),
            ast::Expr::ListExpr(list) => self.lower_list(list, decl_id, path),
            ast::Expr::EveryExpr(every) => {
                let blame = self.make_blame(decl_id, path);
                desugar::desugar_every(self.db, every, &blame).unwrap_or(Value::Undefined)
            }
            ast::Expr::ParenExpr(paren) => match paren.inner() {
                Some(inner) => self.lower_top_expr(&inner, decl_id, path),
                None => Value::Undefined,
            },
            // r[impl expr.literal.identifier]
            ast::Expr::IdentExpr(ident) => {
                if let Some(name_tok) = ident.name() {
                    let name = name_tok.text();
                    // Scan env back-to-front for shadowing
                    for (k, v) in self.env.iter().rev() {
                        if k == name {
                            return v.clone();
                        }
                    }
                    self.emit_diagnostic(
                        name_tok.text_range(),
                        format!("undefined variable `{name}`"),
                    );
                    Value::Undefined
                } else {
                    Value::Undefined
                }
            }
            // r[impl expr.let.scope+2]
            // r[impl expr.let.sequential]
            ast::Expr::LetExpr(let_expr) => {
                let name = let_expr
                    .name()
                    .map(|t| t.text().to_string())
                    .unwrap_or_default();
                let bound_value = match let_expr.bound_expr() {
                    Some(e) => self.lower_top_expr(&e, decl_id, path),
                    None => Value::Undefined,
                };
                self.env.push((name, bound_value));
                let body_value = match let_expr.body_expr() {
                    Some(e) => self.lower_top_expr(&e, decl_id, path),
                    None => Value::Undefined,
                };
                self.env.pop();
                body_value
            }
            ast::Expr::BinaryExpr(bin) => {
                let lhs = match bin.lhs() {
                    Some(e) => self.lower_top_expr(&e, decl_id, path),
                    None => Value::Undefined,
                };
                let rhs = match bin.rhs() {
                    Some(e) => self.lower_top_expr(&e, decl_id, path),
                    None => Value::Undefined,
                };
                let op = bin.op().map(|t| t.kind());
                match op {
                    // r[impl expr.op.concat]
                    Some(SyntaxKind::PLUS_PLUS) => {
                        // List concatenation
                        match (lhs, rhs) {
                            (Value::List(mut a), Value::List(b)) => {
                                a.extend(b);
                                Value::List(a)
                            }
                            (l, r) => {
                                if let Some(op_tok) = bin.op() {
                                    self.emit_diagnostic(
                                        op_tok.text_range(),
                                        format!(
                                            "++ requires two lists, got {} and {}",
                                            l.type_name(),
                                            r.type_name()
                                        ),
                                    );
                                }
                                Value::Undefined
                            }
                        }
                    }
                    // r[impl expr.op.merge]
                    Some(SyntaxKind::SLASH_SLASH) => {
                        // Record merge (right wins)
                        match (lhs, rhs) {
                            (Value::Record(mut a), Value::Record(b)) => {
                                for (k, v) in b.into_iter() {
                                    a.insert(self.db, k, v);
                                }
                                Value::Record(a)
                            }
                            (l, r) => {
                                if let Some(op_tok) = bin.op() {
                                    self.emit_diagnostic(
                                        op_tok.text_range(),
                                        format!(
                                            "// requires two records, got {} and {}",
                                            l.type_name(),
                                            r.type_name()
                                        ),
                                    );
                                }
                                Value::Undefined
                            }
                        }
                    }
                    // r[impl expr.op.eq]
                    Some(SyntaxKind::EQ_EQ) => Value::Bool(values_equal(&lhs, &rhs)),
                    Some(SyntaxKind::BANG_EQ) => Value::Bool(!values_equal(&lhs, &rhs)),
                    _ => Value::Undefined,
                }
            }
            // r[impl expr.op.field]
            ast::Expr::FieldAccessExpr(fa) => {
                let target = match fa.target() {
                    Some(e) => self.lower_top_expr(&e, decl_id, path),
                    None => return Value::Undefined,
                };
                let field_name_str = match fa.field_name() {
                    Some(t) => t.text().to_string(),
                    None => return Value::Undefined,
                };
                match target {
                    Value::Record(r) => {
                        let key = self.intern(&field_name_str);
                        r.get(self.db, &key)
                            .map(|b| b.value.clone())
                            .unwrap_or(Value::Undefined)
                    }
                    _ => {
                        if let Some(name_tok) = fa.field_name() {
                            self.emit_diagnostic(
                                name_tok.text_range(),
                                format!("cannot access field `{field_name_str}` on non-record"),
                            );
                        }
                        Value::Undefined
                    }
                }
            }
            // r[impl expr.op.index]
            ast::Expr::IndexExpr(idx) => {
                let target = match idx.target() {
                    Some(e) => self.lower_top_expr(&e, decl_id, path),
                    None => return Value::Undefined,
                };
                let index_val = match idx.index_expr() {
                    Some(e) => self.lower_top_expr(&e, decl_id, path),
                    None => return Value::Undefined,
                };
                match (target, index_val) {
                    (Value::List(items), Value::Integer(i)) => usize::try_from(i)
                        .ok()
                        .and_then(|idx| items.get(idx))
                        .map(|b| b.value.clone())
                        .unwrap_or(Value::Undefined),
                    _ => Value::Undefined,
                }
            }
            ast::Expr::ImportExpr(import) => self.lower_import(import),
            // r[impl decl.calendar.desugar+2]
            ast::Expr::CalendarExpr(cal) => {
                let mut record = match cal.body() {
                    Some(body) => self.lower_record(&body, decl_id, path),
                    None => Record::new(),
                };
                self.insert_field(
                    &mut record,
                    "type",
                    Value::String("calendar".into()),
                    decl_id,
                    path,
                );
                Value::Record(record)
            }
            // r[impl model.entry.type.infer+2]
            // r[impl decl.event.desugar+2]
            ast::Expr::EventExpr(ev) => {
                let mut record = self.lower_event(ev, decl_id);
                self.insert_field(
                    &mut record,
                    "type",
                    Value::String("event".into()),
                    decl_id,
                    path,
                );
                Value::Record(record)
            }
            // r[impl model.entry.type.infer+2]
            // r[impl decl.task.desugar+2]
            ast::Expr::TaskExpr(task) => {
                let mut record = self.lower_task(task, decl_id);
                self.insert_field(
                    &mut record,
                    "type",
                    Value::String("task".into()),
                    decl_id,
                    path,
                );
                Value::Record(record)
            }
        }
    }

    fn lower_literal(
        &mut self,
        lit: &ast::LiteralExpr,
        decl_id: DeclId<'db>,
        path: &FieldPath<'db>,
    ) -> Option<Value<'db>> {
        let token = lit.literal_token()?;
        let text = token.text();
        let blame = self.make_blame(decl_id, path);

        match token.kind() {
            SyntaxKind::INTEGER_LITERAL => {
                let n = literals::eval_integer(text)?;
                Some(Value::Integer(n))
            }
            SyntaxKind::SIGNED_INTEGER_LITERAL => {
                let n = literals::eval_signed_integer(text)?;
                Some(Value::SignedInteger(n))
            }
            SyntaxKind::STRING_LITERAL => Some(Value::String(literals::eval_string(text))),
            SyntaxKind::TRIPLE_STRING_LITERAL => {
                Some(Value::String(literals::eval_triple_string(text)))
            }
            SyntaxKind::TRUE_KW => Some(Value::Bool(true)),
            SyntaxKind::FALSE_KW => Some(Value::Bool(false)),
            SyntaxKind::UNDEFINED_KW => Some(Value::Undefined),
            SyntaxKind::NAME => Some(Value::Name(literals::eval_name(text))),
            SyntaxKind::DATE_LITERAL => desugar::desugar_date(self.db, text, &blame),
            SyntaxKind::MONTH_DAY_LITERAL => desugar::desugar_month_day(self.db, text, &blame),
            SyntaxKind::TIME_LITERAL => desugar::desugar_time(self.db, text, &blame),
            SyntaxKind::DATETIME_LITERAL => desugar::desugar_datetime(self.db, text, &blame),
            SyntaxKind::DURATION_LITERAL => desugar::desugar_duration(self.db, text, &blame),
            SyntaxKind::URI_LITERAL => Some(Value::String(literals::eval_uri(text))),
            SyntaxKind::ATOM_LITERAL => Some(Value::String(literals::eval_atom(text))),
            SyntaxKind::PATH_LITERAL => Some(Value::Path(text.to_string())),
            _ => {
                self.emit_diagnostic(
                    token.text_range(),
                    format!("unexpected literal token kind: {:?}", token.kind()),
                );
                None
            }
        }
    }

    // r[impl expr.list.syntax]
    fn lower_list(
        &mut self,
        list: &ast::ListExpr,
        decl_id: DeclId<'db>,
        base_path: &FieldPath<'db>,
    ) -> Value<'db> {
        let mut items = Vec::new();
        for (i, elem) in list.elements().enumerate() {
            let elem_path = base_path.index(self.db,i);
            let blame = self.make_blame(decl_id, &elem_path);
            let value = self.lower_top_expr(&elem, decl_id, &elem_path);
            items.push(Blamed { value, blame });
        }
        Value::List(items)
    }

    fn lower_short_dt(
        &mut self,
        dt: &ast::ShortDt,
        decl_id: DeclId<'db>,
        base_path: &FieldPath<'db>,
        field_name: &str,
    ) -> Option<Value<'db>> {
        let blame = self.make_blame(decl_id, &base_path.field(self.db,self.intern(field_name)));
        if let Some(datetime_token) = dt.datetime() {
            desugar::desugar_datetime(self.db, datetime_token.text(), &blame)
        } else {
            let date_token = dt.date()?;
            let time_token = dt.time()?;
            desugar::desugar_date_and_time(self.db, date_token.text(), time_token.text(), &blame)
        }
    }

    // ── Import ───────────────────────────────────────────────────

    // r[impl expr.import.eval]
    // r[impl expr.import.eager]
    fn lower_import(&mut self, import: &ast::ImportExpr) -> Value<'db> {
        let source_token = match import.source() {
            Some(t) => t,
            None => return Value::Undefined,
        };

        // r[impl expr.import.format+2]
        // Determine the import format: explicit keyword or inferred from extension.
        let format = if let Some(fmt_tok) = import.format() {
            match fmt_tok.kind() {
                SyntaxKind::GNOMON_KW => ImportFormat::Gnomon,
                SyntaxKind::ICALENDAR_KW => ImportFormat::ICalendar,
                SyntaxKind::JSCALENDAR_KW => ImportFormat::JSCalendar,
                _ => {
                    self.emit_diagnostic(
                        fmt_tok.text_range(),
                        format!("unknown import format `{}`", fmt_tok.text()),
                    );
                    return Value::Undefined;
                }
            }
        } else {
            ImportFormat::Infer
        };

        let is_uri = source_token.kind() == SyntaxKind::URI_LITERAL;

        let path_str = match source_token.kind() {
            SyntaxKind::PATH_LITERAL => source_token.text().to_string(),
            SyntaxKind::URI_LITERAL => literals::eval_uri(source_token.text()),
            _ => return Value::Undefined,
        };

        // Infer format from extension if not specified.
        let format = match format {
            ImportFormat::Infer => {
                if path_str.ends_with(".ics") {
                    ImportFormat::ICalendar
                } else if path_str.ends_with(".json") {
                    ImportFormat::JSCalendar
                } else {
                    ImportFormat::Infer
                }
            }
            f => f,
        };

        if is_uri {
            self.lower_import_uri(&path_str, format, source_token.text_range())
        } else {
            self.lower_import_path(&path_str, format, source_token.text_range())
        }
    }

    // r[impl expr.import.format.uri]
    fn lower_import_uri(
        &mut self,
        url: &str,
        format: ImportFormat,
        source_range: rowan::TextRange,
    ) -> Value<'db> {
        // Check the on-disk cache first (unless force_refresh is set).
        if !self.force_refresh
            && let super::cache::CacheLookup::Hit {
                content,
                content_type,
            } = super::cache::lookup(url)
        {
            let format = match format {
                ImportFormat::Infer => infer_format_from_content_type(&content_type),
                f => f,
            };
            return self.dispatch_import_content(format, content, PathBuf::from(url), source_range);
        }

        // Fetch content via HTTP(S).
        let response = match ureq::get(url).call() {
            Ok(resp) => resp,
            Err(e) => {
                self.emit_diagnostic(source_range, format!("URI import failed: {e}"));
                return Value::Undefined;
            }
        };

        // If format is still Infer, try Content-Type header.
        let content_type_str = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let format = match format {
            ImportFormat::Infer => infer_format_from_content_type(&content_type_str),
            f => f,
        };

        let content = match response.into_body().read_to_string() {
            Ok(s) => s,
            Err(e) => {
                self.emit_diagnostic(
                    source_range,
                    format!("URI import: failed to read response body: {e}"),
                );
                return Value::Undefined;
            }
        };

        // Store in cache for future lookups.
        let format_hint = match format {
            ImportFormat::ICalendar => "icalendar",
            ImportFormat::JSCalendar => "jscalendar",
            ImportFormat::Gnomon | ImportFormat::Infer => "gnomon",
        };
        super::cache::store(url, &content, &content_type_str, format_hint);

        self.dispatch_import_content(format, content, PathBuf::from(url), source_range)
    }

    fn lower_import_path(
        &mut self,
        path_str: &str,
        format: ImportFormat,
        source_range: rowan::TextRange,
    ) -> Value<'db> {
        // Default Infer to Gnomon for local paths (extension check already ran).
        let format = match format {
            ImportFormat::Infer => ImportFormat::Gnomon,
            f => f,
        };

        // r[impl lexer.path.relative]
        // r[impl model.import.resolution]
        // Resolve relative to the directory containing the importing file.
        let base_dir = self
            .source
            .path(self.db)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let target_path = base_dir.join(path_str);

        // Normalize the path for cycle detection (canonicalize if possible,
        // otherwise use the joined path as-is).
        let target_path = target_path.canonicalize().unwrap_or(target_path);

        // r[impl expr.import.cycle]
        if self.import_stack.contains(&target_path) {
            self.emit_diagnostic(
                source_range,
                format!("circular import detected: {}", target_path.display()),
            );
            return Value::Undefined;
        }

        // Read the target file.
        let content = match std::fs::read_to_string(&target_path) {
            Ok(c) => c,
            Err(e) => {
                self.emit_diagnostic(
                    source_range,
                    format!("cannot read import `{}`: {e}", target_path.display()),
                );
                return Value::Undefined;
            }
        };

        self.dispatch_import_content(format, content, target_path, source_range)
    }

    // r[impl model.import.transparent]
    fn dispatch_import_content(
        &mut self,
        format: ImportFormat,
        content: String,
        source_path: PathBuf,
        source_range: rowan::TextRange,
    ) -> Value<'db> {
        match format {
            ImportFormat::Gnomon => {
                let target_source = SourceFile::new(self.db, source_path.clone(), content);
                let mut new_stack = self.import_stack.clone();
                new_stack.push(source_path.clone());

                let result = super::evaluate_with_import_stack(
                    self.db,
                    target_source,
                    new_stack,
                    self.force_refresh,
                );

                self.diagnostics.extend(result.diagnostics);
                self.imported_files.push(source_path);
                self.imported_files.extend(result.imported_files);
                result.value
            }
            ImportFormat::ICalendar => {
                let decl_id = self.make_decl_id(0, DeclKind::Expr);
                let blame = self.make_blame(decl_id, &FieldPath::root(self.db));
                match super::import::translate_icalendar(self.db, &content, &blame) {
                    Ok(value) => value,
                    Err(msg) => {
                        self.emit_diagnostic(source_range, msg);
                        Value::Undefined
                    }
                }
            }
            ImportFormat::JSCalendar => {
                let decl_id = self.make_decl_id(0, DeclKind::Expr);
                let blame = self.make_blame(decl_id, &FieldPath::root(self.db));
                match super::import::translate_jscalendar(self.db, &content, &blame) {
                    Ok(value) => value,
                    Err(msg) => {
                        self.emit_diagnostic(source_range, msg);
                        Value::Undefined
                    }
                }
            }
            ImportFormat::Infer => unreachable!(),
        }
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn intern(&self, text: &str) -> FieldName<'db> {
        FieldName::new(self.db, text.to_string())
    }

    fn make_decl_id(&self, index: usize, kind: DeclKind) -> DeclId<'db> {
        DeclId::new(self.db, self.source, index, kind)
    }

    fn root_blame(&self, decl_id: DeclId<'db>) -> Blame<'db> {
        Blame {
            decl: decl_id,
            path: FieldPath::root(self.db),
        }
    }

    fn make_blame(&self, decl_id: DeclId<'db>, path: &FieldPath<'db>) -> Blame<'db> {
        Blame {
            decl: decl_id,
            path: path.clone(),
        }
    }

    fn insert_field(
        &self,
        record: &mut Record<'db>,
        name: &str,
        value: Value<'db>,
        decl_id: DeclId<'db>,
        base_path: &FieldPath<'db>,
    ) {
        let field_name = self.intern(name);
        let blame = self.make_blame(decl_id, &base_path.field(self.db,field_name));
        record.insert(self.db, field_name, Blamed { value, blame });
    }

    fn emit_diagnostic(&mut self, range: rowan::TextRange, message: String) {
        self.diagnostics.push(Diagnostic {
            source: self.source,
            range,
            severity: crate::queries::Severity::Error,
            message,
        });
    }
}

fn decl_kind_for_expr(expr: &ast::Expr) -> DeclKind {
    match expr {
        ast::Expr::CalendarExpr(_) => DeclKind::Calendar,
        ast::Expr::EventExpr(_) => DeclKind::Event,
        ast::Expr::TaskExpr(_) => DeclKind::Task,
        _ => DeclKind::Expr,
    }
}

/// Structural equality for values (used by `==` and `!=`).
///
/// Compares values by their content, ignoring blame metadata on nested
/// `Blamed` wrappers so that e.g. `[1] ++ [2] == [1, 2]` is `true`.
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Integer(a), Value::Integer(b)) => a == b,
        (Value::SignedInteger(a), Value::SignedInteger(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Undefined, Value::Undefined) => true,
        (Value::Name(a), Value::Name(b)) => a == b,
        (Value::Path(a), Value::Path(b)) => a == b,
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(x, y)| values_equal(&x.value, &y.value))
        }
        (Value::Record(a), Value::Record(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|((ka, va), (kb, vb))| ka == kb && values_equal(&va.value, &vb.value))
        }
        _ => false,
    }
}
