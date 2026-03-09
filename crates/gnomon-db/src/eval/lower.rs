use std::path::PathBuf;

use gnomon_parser::ast;
use gnomon_parser::SyntaxKind;

use super::desugar;
use super::interned::{DeclId, DeclKind, FieldName, FieldPath};
use super::literals;
use super::types::{Blame, Blamed, Record, Value};
use crate::input::SourceFile;
use crate::queries::Diagnostic;

#[derive(Debug, Clone, Copy)]
enum ImportFormat {
    Gnomon,
    ICalendar,
    JSCalendar,
    Infer,
}

pub struct LowerCtx<'db> {
    db: &'db dyn crate::Db,
    source: SourceFile,
    pub diagnostics: Vec<Diagnostic>,
    /// Environment for let bindings (name, value). Scanned back-to-front for shadowing.
    env: Vec<(String, Value<'db>)>,
    /// Stack of file paths currently being evaluated, for cycle detection.
    import_stack: Vec<PathBuf>,
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
        }
    }

    /// Lower a source file into a Value.
    ///
    /// File structure: optional `let` bindings, then either declarations or an expression body.
    pub fn lower_source_file(&mut self, file: &ast::SourceFile) -> Value<'db> {
        // 1. Process file-level let bindings (pushed into env).
        for binding in file.let_bindings() {
            if let Some(name_tok) = binding.name() {
                let name = name_tok.text().to_string();
                let decl_id = self.make_decl_id(0, DeclKind::Expr);
                let value = match binding.value_expr() {
                    Some(expr) => self.lower_top_expr(&expr, decl_id, &FieldPath::root()),
                    None => Value::Undefined,
                };
                self.env.push((name, value));
            }
        }

        // 2. Evaluate body
        let decls: Vec<_> = file.decls().collect();
        if !decls.is_empty() {
            // Declaration mode
            if decls.len() == 1 {
                self.lower_decl_to_value(&decls[0], 0)
            } else {
                let items: Vec<_> = decls
                    .iter()
                    .enumerate()
                    .map(|(i, decl)| {
                        let value = self.lower_decl_to_value(decl, i);
                        let did = self.make_decl_id(i, match decl {
                            ast::Decl::CalendarDecl(_) => DeclKind::Calendar,
                            ast::Decl::EventDecl(_) => DeclKind::Event,
                            ast::Decl::TaskDecl(_) => DeclKind::Task,
                        });
                        Blamed {
                            value,
                            blame: self.root_blame(did),
                        }
                    })
                    .collect();
                Value::List(items)
            }
        } else if let Some(body) = file.body_expr() {
            let decl_id = self.make_decl_id(0, DeclKind::Expr);
            self.lower_top_expr(&body, decl_id, &FieldPath::root())
        } else {
            Value::Undefined
        }
    }

    /// Lower a declaration to a Value.
    fn lower_decl_to_value(&mut self, decl: &ast::Decl, index: usize) -> Value<'db> {
        match decl {
            ast::Decl::CalendarDecl(cal) => {
                let decl_id = self.make_decl_id(index, DeclKind::Calendar);
                match cal.body() {
                    Some(body) => {
                        Value::Record(self.lower_record(&body, decl_id, &FieldPath::root()))
                    }
                    None => Value::Record(Record::new()),
                }
            }
            // r[impl model.entry.type.infer]
            ast::Decl::EventDecl(ev) => {
                let decl_id = self.make_decl_id(index, DeclKind::Event);
                let mut record = self.lower_event(ev, decl_id);
                self.insert_field(
                    &mut record,
                    "type",
                    Value::String("event".into()),
                    decl_id,
                    &FieldPath::root(),
                );
                Value::Record(record)
            }
            // r[impl model.entry.type.infer]
            ast::Decl::TaskDecl(task) => {
                let decl_id = self.make_decl_id(index, DeclKind::Task);
                let mut record = self.lower_task(task, decl_id);
                self.insert_field(
                    &mut record,
                    "type",
                    Value::String("task".into()),
                    decl_id,
                    &FieldPath::root(),
                );
                Value::Record(record)
            }
        }
    }

    fn lower_event(&mut self, ev: &ast::EventDecl, decl_id: DeclId<'db>) -> Record<'db> {
        let base_path = FieldPath::root();

        if ev.name().is_some() {
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
                    let blame = self.make_blame(decl_id, &base_path.field(self.intern("duration")));
                    if let Some(value) = desugar::desugar_duration(self.db, dur_token.text(), &blame)
                    {
                        self.insert_field(&mut record, "duration", value, decl_id, &base_path);
                    }
                }
            }

            if let Some(title_token) = ev.title() {
                let title = literals::eval_string(title_token.text());
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
                for (name, value) in body_record.0 {
                    record.0.insert(name, value);
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

    fn lower_task(&mut self, task: &ast::TaskDecl, decl_id: DeclId<'db>) -> Record<'db> {
        let base_path = FieldPath::root();

        if task.name().is_some() {
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
                let title = literals::eval_string(title_token.text());
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
                for (name, value) in body_record.0 {
                    record.0.insert(name, value);
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
            let field_path = base_path.field(field_name);
            let blame = self.make_blame(decl_id, &field_path);

            let value = self.lower_top_expr(&value_expr, decl_id, &field_path);
            result.insert(field_name, Blamed { value, blame });
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
            ast::Expr::LiteralExpr(lit) => {
                self.lower_literal(lit, decl_id, path)
                    .unwrap_or(Value::Undefined)
            }
            ast::Expr::RecordExpr(rec) => {
                Value::Record(self.lower_record(rec, decl_id, path))
            }
            ast::Expr::ListExpr(list) => self.lower_list(list, decl_id, path),
            ast::Expr::EveryExpr(every) => {
                let blame = self.make_blame(decl_id, path);
                desugar::desugar_every(self.db, every, &blame)
                    .unwrap_or(Value::Undefined)
            }
            ast::Expr::ParenExpr(paren) => {
                match paren.inner() {
                    Some(inner) => self.lower_top_expr(&inner, decl_id, path),
                    None => Value::Undefined,
                }
            }
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
            ast::Expr::LetExpr(let_expr) => {
                let name = let_expr.name().map(|t| t.text().to_string()).unwrap_or_default();
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
                                            value_type_name(&l),
                                            value_type_name(&r)
                                        ),
                                    );
                                }
                                Value::Undefined
                            }
                        }
                    }
                    Some(SyntaxKind::SLASH_SLASH) => {
                        // Record merge (right wins)
                        match (lhs, rhs) {
                            (Value::Record(mut a), Value::Record(b)) => {
                                for (k, v) in b.0 {
                                    a.0.insert(k, v);
                                }
                                Value::Record(a)
                            }
                            (l, r) => {
                                if let Some(op_tok) = bin.op() {
                                    self.emit_diagnostic(
                                        op_tok.text_range(),
                                        format!(
                                            "// requires two records, got {} and {}",
                                            value_type_name(&l),
                                            value_type_name(&r)
                                        ),
                                    );
                                }
                                Value::Undefined
                            }
                        }
                    }
                    Some(SyntaxKind::EQ_EQ) => {
                        Value::Bool(values_equal(&lhs, &rhs))
                    }
                    Some(SyntaxKind::BANG_EQ) => {
                        Value::Bool(!values_equal(&lhs, &rhs))
                    }
                    _ => Value::Undefined,
                }
            }
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
                        r.get(&key).map(|b| b.value.clone()).unwrap_or(Value::Undefined)
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
                    (Value::List(items), Value::Integer(i)) => {
                        items.get(i as usize).map(|b| b.value.clone()).unwrap_or(Value::Undefined)
                    }
                    _ => Value::Undefined,
                }
            }
            ast::Expr::ImportExpr(import) => {
                self.lower_import(import)
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
            SyntaxKind::STRING_LITERAL => {
                Some(Value::String(literals::eval_string(text)))
            }
            SyntaxKind::TRUE_KW => Some(Value::Bool(true)),
            SyntaxKind::FALSE_KW => Some(Value::Bool(false)),
            SyntaxKind::UNDEFINED_KW => Some(Value::Undefined),
            SyntaxKind::NAME => {
                Some(Value::Name(literals::eval_name(text)))
            }
            SyntaxKind::DATE_LITERAL => {
                desugar::desugar_date(self.db, text, &blame)
            }
            SyntaxKind::MONTH_DAY_LITERAL => {
                desugar::desugar_month_day(self.db, text, &blame)
            }
            SyntaxKind::TIME_LITERAL => {
                desugar::desugar_time(self.db, text, &blame)
            }
            SyntaxKind::DATETIME_LITERAL => {
                desugar::desugar_datetime(self.db, text, &blame)
            }
            SyntaxKind::DURATION_LITERAL => {
                desugar::desugar_duration(self.db, text, &blame)
            }
            SyntaxKind::URI_LITERAL => {
                Some(Value::String(literals::eval_uri(text)))
            }
            SyntaxKind::ATOM_LITERAL => {
                Some(Value::String(literals::eval_atom(text)))
            }
            SyntaxKind::PATH_LITERAL => {
                Some(Value::Path(text.to_string()))
            }
            _ => {
                self.emit_diagnostic(
                    token.text_range(),
                    format!("unexpected literal token kind: {:?}", token.kind()),
                );
                None
            }
        }
    }

    fn lower_list(
        &mut self,
        list: &ast::ListExpr,
        decl_id: DeclId<'db>,
        base_path: &FieldPath<'db>,
    ) -> Value<'db> {
        let mut items = Vec::new();
        for (i, elem) in list.elements().enumerate() {
            let elem_path = base_path.index(i);
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
        let blame = self.make_blame(decl_id, &base_path.field(self.intern(field_name)));
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

        let path_str = match source_token.kind() {
            SyntaxKind::STRING_LITERAL => literals::eval_string(source_token.text()),
            SyntaxKind::PATH_LITERAL => source_token.text().to_string(),
            SyntaxKind::URI_LITERAL => {
                self.emit_diagnostic(
                    source_token.text_range(),
                    "URI imports are not yet supported".into(),
                );
                return Value::Undefined;
            }
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
                    ImportFormat::Gnomon
                }
            }
            f => f,
        };

        // r[impl lexer.path.relative]
        // Resolve relative to the directory containing the importing file.
        let base_dir = self
            .source
            .path(self.db)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let target_path = base_dir.join(&path_str);

        // Normalize the path for cycle detection (canonicalize if possible,
        // otherwise use the joined path as-is).
        let target_path = target_path.canonicalize().unwrap_or(target_path);

        // r[impl expr.import.cycle]
        if self.import_stack.contains(&target_path) {
            self.emit_diagnostic(
                source_token.text_range(),
                format!("circular import detected: {}", target_path.display()),
            );
            return Value::Undefined;
        }

        // Read the target file.
        let content = match std::fs::read_to_string(&target_path) {
            Ok(c) => c,
            Err(e) => {
                self.emit_diagnostic(
                    source_token.text_range(),
                    format!("cannot read import `{}`: {e}", target_path.display()),
                );
                return Value::Undefined;
            }
        };

        match format {
            ImportFormat::Gnomon => {
                // Create SourceFile input and evaluate.
                let target_source = SourceFile::new(self.db, target_path.clone(), content);
                let mut new_stack = self.import_stack.clone();
                new_stack.push(target_path);

                let result =
                    super::evaluate_with_import_stack(self.db, target_source, new_stack);

                // Collect diagnostics from the imported file.
                self.diagnostics.extend(result.diagnostics);

                result.value
            }
            ImportFormat::ICalendar => {
                let decl_id = self.make_decl_id(0, DeclKind::Expr);
                let blame = self.make_blame(decl_id, &FieldPath::root());
                match super::import::translate_icalendar(self.db, &content, &blame) {
                    Ok(value) => value,
                    Err(msg) => {
                        self.emit_diagnostic(source_token.text_range(), msg);
                        Value::Undefined
                    }
                }
            }
            ImportFormat::JSCalendar => {
                let decl_id = self.make_decl_id(0, DeclKind::Expr);
                let blame = self.make_blame(decl_id, &FieldPath::root());
                match super::import::translate_jscalendar(self.db, &content, &blame) {
                    Ok(value) => value,
                    Err(msg) => {
                        self.emit_diagnostic(source_token.text_range(), msg);
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
            path: FieldPath::root(),
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
        let blame = self.make_blame(decl_id, &base_path.field(field_name));
        record.insert(field_name, Blamed { value, blame });
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

/// Structural equality for values (used by == and !=).
fn values_equal(a: &Value, b: &Value) -> bool {
    a == b
}

fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::SignedInteger(_) => "signed integer",
        Value::Bool(_) => "bool",
        Value::Undefined => "undefined",
        Value::Name(_) => "name",
        Value::Record(_) => "record",
        Value::List(_) => "list",
        Value::Path(_) => "path",
    }
}
