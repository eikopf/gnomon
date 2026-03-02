use std::collections::BTreeMap;

use gnomon_parser::ast;
use gnomon_parser::SyntaxKind;

use super::desugar;
use super::interned::{DeclId, DeclKind, FieldName, FieldPath};
use super::literals;
use super::types::{
    Blame, Blamed, Document, IncludeRef, Name, Record, ReifiedDecl, Uid, Value,
};
use crate::input::SourceFile;
use crate::queries::Diagnostic;

pub struct LowerCtx<'db> {
    db: &'db dyn crate::Db,
    source: SourceFile,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'db> LowerCtx<'db> {
    pub fn new(db: &'db dyn crate::Db, source: SourceFile) -> Self {
        Self {
            db,
            source,
            diagnostics: Vec::new(),
        }
    }

    pub fn lower_document(&mut self, file: &ast::SourceFile) -> Document<'db> {
        let mut bindings: BTreeMap<Name, Blamed<'db, Uid>> = BTreeMap::new();
        let mut decls: Vec<Blamed<'db, ReifiedDecl<'db>>> = Vec::new();

        for (index, decl) in file.decls().enumerate() {
            match decl {
                ast::Decl::InclusionDecl(inc) => {
                    let decl_id = self.make_decl_id(index, DeclKind::Include);
                    if let Some(reified) = self.lower_inclusion(&inc, decl_id) {
                        let blame = self.root_blame(decl_id);
                        decls.push(Blamed {
                            value: reified,
                            blame,
                        });
                    }
                }
                ast::Decl::BindingDecl(bind) => {
                    let decl_id = self.make_decl_id(index, DeclKind::Bind);
                    if let Some((name, blamed_uid)) = self.lower_binding(&bind, decl_id) {
                        bindings.insert(name, blamed_uid);
                    }
                }
                ast::Decl::CalendarDecl(cal) => {
                    let decl_id = self.make_decl_id(index, DeclKind::Calendar);
                    let blame = self.root_blame(decl_id);
                    let record = match cal.body() {
                        Some(body) => self.lower_record(&body, decl_id, &FieldPath::root()),
                        None => Record::new(),
                    };
                    decls.push(Blamed {
                        value: ReifiedDecl::Calendar(record),
                        blame,
                    });
                }
                ast::Decl::EventDecl(ev) => {
                    let decl_id = self.make_decl_id(index, DeclKind::Event);
                    let blame = self.root_blame(decl_id);
                    let record = self.lower_event(&ev, decl_id);
                    decls.push(Blamed {
                        value: ReifiedDecl::Event(record),
                        blame,
                    });
                }
                ast::Decl::TaskDecl(task) => {
                    let decl_id = self.make_decl_id(index, DeclKind::Task);
                    let blame = self.root_blame(decl_id);
                    let record = self.lower_task(&task, decl_id);
                    decls.push(Blamed {
                        value: ReifiedDecl::Task(record),
                        blame,
                    });
                }
            }
        }

        Document { bindings, decls }
    }

    fn lower_inclusion(
        &mut self,
        inc: &ast::InclusionDecl,
        _decl_id: DeclId<'db>,
    ) -> Option<ReifiedDecl<'db>> {
        let path_token = inc.path()?;
        let path_str = literals::eval_string(path_token.text());

        // Detect URI vs local path: if it contains "://" treat as URI.
        let target = if path_str.contains("://") {
            IncludeRef::Uri(path_str)
        } else {
            IncludeRef::Path(path_str.into())
        };

        Some(ReifiedDecl::Include {
            target,
            content: Vec::new(),
        })
    }

    fn lower_binding(
        &mut self,
        bind: &ast::BindingDecl,
        decl_id: DeclId<'db>,
    ) -> Option<(Name, Blamed<'db, Uid>)> {
        let name_token = bind.name()?;
        let path_token = bind.path()?;
        let name = literals::eval_name(name_token.text());
        let uid = literals::eval_string(path_token.text());
        let blame = self.root_blame(decl_id);
        Some((name, Blamed { value: uid, blame }))
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

            // Merge body fields if present.
            if let Some(body) = ev.body() {
                let body_record = self.lower_record(&body, decl_id, &base_path);
                for (name, value) in body_record.0 {
                    record.0.insert(name, value);
                }
            }

            record
        } else {
            // Prefix form: event { ... }
            match ev.body() {
                Some(body) => self.lower_record(&body, decl_id, &base_path),
                None => Record::new(),
            }
        }
    }

    fn lower_task(&mut self, task: &ast::TaskDecl, decl_id: DeclId<'db>) -> Record<'db> {
        let base_path = FieldPath::root();

        if task.name().is_some() {
            // Short form: task @name [datetime] ["title"] [{ body }]
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

            // Merge body fields if present.
            if let Some(body) = task.body() {
                let body_record = self.lower_record(&body, decl_id, &base_path);
                for (name, value) in body_record.0 {
                    record.0.insert(name, value);
                }
            }

            record
        } else {
            // Prefix form: task { ... }
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

            if let Some(value) = self.lower_expr(&value_expr, decl_id, &field_path) {
                result.insert(
                    field_name,
                    Blamed { value, blame },
                );
            }
        }
        result
    }

    fn lower_expr(
        &mut self,
        expr: &ast::Expr,
        decl_id: DeclId<'db>,
        path: &FieldPath<'db>,
    ) -> Option<Value<'db>> {
        match expr {
            ast::Expr::LiteralExpr(lit) => self.lower_literal(lit, decl_id, path),
            ast::Expr::RecordExpr(rec) => {
                Some(Value::Record(self.lower_record(rec, decl_id, path)))
            }
            ast::Expr::ListExpr(list) => Some(self.lower_list(list, decl_id, path)),
            ast::Expr::EveryExpr(every) => {
                let blame = self.make_blame(decl_id, path);
                desugar::desugar_every(self.db, every, &blame)
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
            if let Some(value) = self.lower_expr(&elem, decl_id, &elem_path) {
                items.push(Blamed { value, blame });
            }
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
            range,
            severity: crate::queries::Severity::Error,
            message,
        });
    }
}
