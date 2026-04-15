//! Statement parsing (IEEE 1800-2017 §A.6)

use super::Parser;
use crate::ast::stmt::*;
use crate::ast::expr::{ExprKind, BinaryOp, Expression};
use crate::ast::types::{DataType, TypeName, Lifetime};
use crate::lexer::token::TokenKind;

impl Parser {
    pub(super) fn parse_statement(&mut self) -> Statement {
        let start = self.current().span.start;

        match self.current_kind() {
            TokenKind::Directive => { self.bump(); self.parse_statement() }
            TokenKind::KwBegin => self.parse_seq_block(),
            TokenKind::KwFork => self.parse_par_block(),
            TokenKind::KwIf | TokenKind::KwUnique | TokenKind::KwUnique0 | TokenKind::KwPriority => {
                self.parse_if_or_case()
            }
            TokenKind::KwCase | TokenKind::KwCasex | TokenKind::KwCasez => self.parse_case_statement(),
            TokenKind::KwFor => self.parse_for_statement(),
            TokenKind::KwForeach => self.parse_foreach_statement(),
            TokenKind::KwWhile => self.parse_while_statement(),
            TokenKind::KwDo => self.parse_do_while_statement(),
            TokenKind::KwRepeat => self.parse_repeat_statement(),
            TokenKind::KwForever => {
                self.bump();
                let body = self.parse_statement();
                Statement::new(StatementKind::Forever { body: Box::new(body) }, self.span_from(start))
            }
            TokenKind::KwReturn => {
                self.bump();
                let expr = if !self.at(TokenKind::Semicolon) {
                    Some(self.parse_expression())
                } else { None };
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::Return(expr), self.span_from(start))
            }
            TokenKind::KwBreak => { self.bump(); self.expect(TokenKind::Semicolon); Statement::new(StatementKind::Break, self.span_from(start)) }
            TokenKind::KwContinue => { self.bump(); self.expect(TokenKind::Semicolon); Statement::new(StatementKind::Continue, self.span_from(start)) }
            TokenKind::KwWait => {
                self.bump();
                if self.eat(TokenKind::KwFork).is_some() {
                    self.expect(TokenKind::Semicolon);
                    Statement::new(StatementKind::WaitFork, self.span_from(start))
                } else {
                    self.expect(TokenKind::LParen);
                    let cond = self.parse_expression();
                    self.expect(TokenKind::RParen);
                    let stmt = self.parse_statement();
                    Statement::new(StatementKind::Wait { condition: cond, stmt: Box::new(stmt) }, self.span_from(start))
                }
            }
            TokenKind::KwStatic | TokenKind::KwAutomatic | TokenKind::KwLocal => {
                let mut lifetime = None;
                if self.at(TokenKind::KwStatic) { lifetime = Some(Lifetime::Static); self.bump(); }
                else if self.at(TokenKind::KwAutomatic) { lifetime = Some(Lifetime::Automatic); self.bump(); }
                else if self.at(TokenKind::KwLocal) { self.bump(); } // skip local
                
                if lifetime.is_none() {
                    if self.at(TokenKind::KwStatic) { lifetime = Some(Lifetime::Static); self.bump(); }
                    else if self.at(TokenKind::KwAutomatic) { lifetime = Some(Lifetime::Automatic); self.bump(); }
                }
                let data_type = if self.is_data_type_keyword() || self.at(TokenKind::Identifier) {
                    self.parse_data_type()
                } else {
                    DataType::Implicit { signing: None, dimensions: Vec::new(), span: self.span_from(start) }
                };
                let mut declarators = Vec::new();
                loop {
                    let ds = self.current().span.start;
                    let name = self.parse_identifier();
                    let dimensions = self.parse_unpacked_dimensions();
                    let init = if self.eat(TokenKind::Assign).is_some() {
                        Some(self.parse_expression())
                    } else { None };
                    declarators.push(VarDeclarator { name, dimensions, init, span: self.span_from(ds) });
                    if self.eat(TokenKind::Comma).is_none() { break; }
                }
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::VarDecl { data_type, lifetime, declarators }, self.span_from(start))
            }
            TokenKind::KwTypedef => {
                let _ = self.parse_typedef_declaration();
                Statement::new(StatementKind::Null, self.span_from(start))
            }
            TokenKind::KwDisable => {
                self.bump();
                if self.eat(TokenKind::KwFork).is_some() {
                    self.expect(TokenKind::Semicolon);
                    Statement::new(StatementKind::Null, self.span_from(start))
                } else {
                    let name = self.parse_identifier();
                    self.expect(TokenKind::Semicolon);
                    Statement::new(StatementKind::Disable(name), self.span_from(start))
                }
            }
            TokenKind::KwAssert | TokenKind::KwAssume | TokenKind::KwCover => {
                Statement::new(StatementKind::Assertion(self.parse_assertion_statement()), self.span_from(start))
            }
            TokenKind::KwAssign => {
                self.bump();
                let lv = self.parse_expression();
                self.expect(TokenKind::Assign);
                let rv = self.parse_expression();
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::ProceduralContinuous(
                    ProceduralContinuous::Assign { lvalue: lv, rvalue: rv }
                ), self.span_from(start))
            }
            TokenKind::KwForce => {
                self.bump();
                let lv = self.parse_expression();
                self.expect(TokenKind::Assign);
                let rv = self.parse_expression();
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::ProceduralContinuous(
                    ProceduralContinuous::Force { lvalue: lv, rvalue: rv }
                ), self.span_from(start))
            }
            TokenKind::KwDeassign => {
                self.bump();
                let lv = self.parse_expression();
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::ProceduralContinuous(
                    ProceduralContinuous::Deassign(lv)
                ), self.span_from(start))
            }
            TokenKind::KwCoverpoint => {
                self.bump();
                let expr = self.parse_expression();
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::Coverpoint { name: None, expr, span: self.span_from(start) }, self.span_from(start))
            }
            TokenKind::KwCross => {
                self.bump();
                let mut items = Vec::new();
                loop {
                    items.push(self.parse_expression());
                    if self.eat(TokenKind::Comma).is_none() { break; }
                }
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::Cross { name: None, items, span: self.span_from(start) }, self.span_from(start))
            }
            TokenKind::KwRelease => {
                self.bump();
                let lv = self.parse_expression();
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::ProceduralContinuous(
                    ProceduralContinuous::Release(lv)
                ), self.span_from(start))
            }
            // Timing control: @
            TokenKind::At => {
                let ctrl = self.parse_event_control();
                let stmt = self.parse_statement();
                Statement::new(StatementKind::TimingControl {
                    control: TimingControl::Event(ctrl),
                    stmt: Box::new(stmt),
                }, self.span_from(start))
            }
            // Event trigger: ->, ->>
            TokenKind::Arrow | TokenKind::DoubleArrow => {
                let nonblocking = self.bump().kind == TokenKind::DoubleArrow;
                let target = self.parse_expression();
                self.expect(TokenKind::Semicolon);
                let name = match target.kind {
                    ExprKind::Ident(hier) => {
                        hier.path.last().map(|seg| seg.name.clone()).unwrap_or_else(|| crate::ast::Identifier {
                            name: "event".to_string(),
                            span: self.span_from(start),
                        })
                    }
                    _ => crate::ast::Identifier {
                        name: "event".to_string(),
                        span: self.span_from(start),
                    },
                };
                Statement::new(StatementKind::EventTrigger { nonblocking, name, span: self.span_from(start) }, self.span_from(start))
            }
            // Delay control: #
            TokenKind::Hash => {
                self.bump();
                let delay = self.parse_expression();
                let stmt = self.parse_statement();
                Statement::new(StatementKind::TimingControl {
                    control: TimingControl::Delay(delay),
                    stmt: Box::new(stmt),
                }, self.span_from(start))
            }
            // Variable declaration (data type keywords)
            k if self.is_data_type_keyword() && k != TokenKind::KwEvent &&
                 !(self.peek_kind() == TokenKind::IntegerLiteral && {
                     let next_text = self.tokens.get(self.pos + 1).map(|t| t.text.as_str()).unwrap_or("");
                     next_text == "'"
                 }) => {
                let data_type = self.parse_data_type();
                let lifetime = None;
                let mut declarators = Vec::new();
                loop {
                    let ds = self.current().span.start;
                    let name = self.parse_identifier();
                    let dimensions = self.parse_unpacked_dimensions();
                    let init = if self.eat(TokenKind::Assign).is_some() {
                        Some(self.parse_expression())
                    } else { None };
                    declarators.push(VarDeclarator { name, dimensions, init, span: self.span_from(ds) });
                    if self.eat(TokenKind::Comma).is_none() { break; }
                }
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::VarDecl { data_type, lifetime, declarators }, self.span_from(start))
            }
            TokenKind::KwInput | TokenKind::KwOutput | TokenKind::KwInout | TokenKind::KwRef => {
                let start = self.current().span.start;
                self.bump();
                while !self.at(TokenKind::Semicolon) && !self.at(TokenKind::Eof) { self.bump(); }
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::Null, self.span_from(start))
            }
            // Null statement
            TokenKind::Semicolon => {
                self.bump();
                Statement::new(StatementKind::Null, self.span_from(start))
            }
            // Event declaration
            TokenKind::KwEvent => {
                self.bump();
                let mut names = Vec::new();
                loop {
                    names.push(self.parse_identifier());
                    if self.eat(TokenKind::Comma).is_none() { break; }
                }
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::Null, self.span_from(start)) // Skip for now
            }
            // User-defined type variable declaration: TypeName var [= expr];
            // Detected by: Identifier followed by Identifier, Hash (if followed by identifier), 
            // or DoubleColon (if followed by identifier).
            // Expressions starting with Identifier: class_scope::member, pkg::member, obj.member
            TokenKind::Identifier if !self.peek_is_class_scope() && matches!(self.peek_kind(), TokenKind::Identifier | TokenKind::Hash | TokenKind::DoubleColon) =>
            {
                let data_type = self.parse_data_type();
                let mut declarators = Vec::new();
                loop {
                    let ds = self.current().span.start;
                    let name = self.parse_identifier();
                    let dimensions = self.parse_unpacked_dimensions();
                    let init = if self.eat(TokenKind::Assign).is_some() {
                        Some(self.parse_expression())
                    } else { None };
                    declarators.push(VarDeclarator { name, dimensions, init, span: self.span_from(ds) });
                    if self.eat(TokenKind::Comma).is_none() { break; }
                }
                self.expect(TokenKind::Semicolon);
                Statement::new(StatementKind::VarDecl { data_type, lifetime: None, declarators }, self.span_from(start))
            }
            // Expression statement (assignment, call, inc/dec)
            _ => {
                // Parse LHS expression, but stop at <= to allow nonblocking assignment
                let expr = self.parse_lvalue_or_expr();
                // Check for blocking/nonblocking assignment
                if self.at(TokenKind::Assign) || self.at_any(&[
                    TokenKind::PlusAssign, TokenKind::MinusAssign,
                    TokenKind::StarAssign, TokenKind::SlashAssign,
                    TokenKind::PercentAssign, TokenKind::AndAssign,
                    TokenKind::OrAssign, TokenKind::XorAssign,
                    TokenKind::ShiftLeftAssign, TokenKind::ShiftRightAssign,
                ]) {
                    let op_kind = self.current().kind.clone();
                    self.bump();
                    let rhs = self.parse_expression();
                    self.expect(TokenKind::Semicolon);
                    // Expand compound assignments: lhs += rhs => lhs = lhs + rhs
                    let rvalue = match op_kind {
                        TokenKind::PlusAssign => Expression::new(ExprKind::Binary { op: BinaryOp::Add, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::MinusAssign => Expression::new(ExprKind::Binary { op: BinaryOp::Sub, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::StarAssign => Expression::new(ExprKind::Binary { op: BinaryOp::Mul, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::SlashAssign => Expression::new(ExprKind::Binary { op: BinaryOp::Div, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::PercentAssign => Expression::new(ExprKind::Binary { op: BinaryOp::Mod, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::AndAssign => Expression::new(ExprKind::Binary { op: BinaryOp::BitAnd, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::OrAssign => Expression::new(ExprKind::Binary { op: BinaryOp::BitOr, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::XorAssign => Expression::new(ExprKind::Binary { op: BinaryOp::BitXor, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::ShiftLeftAssign => Expression::new(ExprKind::Binary { op: BinaryOp::ShiftLeft, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        TokenKind::ShiftRightAssign => Expression::new(ExprKind::Binary { op: BinaryOp::ShiftRight, left: Box::new(expr.clone()), right: Box::new(rhs) }, self.span_from(start)),
                        _ => rhs, // TokenKind::Assign - plain assignment
                    };
                    Statement::new(StatementKind::BlockingAssign { lvalue: expr, rvalue }, self.span_from(start))
                } else if self.at(TokenKind::Leq) {
                    // Nonblocking assignment: lvalue <= rvalue
                    self.bump();
                    let rvalue = self.parse_expression();
                    self.expect(TokenKind::Semicolon);
                    Statement::new(StatementKind::NonblockingAssign {
                        lvalue: expr, delay: None, rvalue,
                    }, self.span_from(start))
                } else {
                    self.expect(TokenKind::Semicolon);
                    Statement::new(StatementKind::Expr(expr), self.span_from(start))
                }
            }
        }
    }

    fn parse_seq_block(&mut self) -> Statement {
        let start = self.current().span.start;
        self.expect(TokenKind::KwBegin);
        let name = if self.eat(TokenKind::Colon).is_some() {
            Some(self.parse_identifier())
        } else { None };
        let mut stmts = Vec::new();
        while !self.at(TokenKind::KwEnd) && !self.at(TokenKind::Eof) {
            stmts.push(self.parse_statement());
        }
        self.expect(TokenKind::KwEnd);
        let _ = self.parse_end_label();
        Statement::new(StatementKind::SeqBlock { name, stmts }, self.span_from(start))
    }

    fn parse_par_block(&mut self) -> Statement {
        let start = self.current().span.start;
        self.expect(TokenKind::KwFork);
        let name = if self.eat(TokenKind::Colon).is_some() {
            Some(self.parse_identifier())
        } else { None };
        let mut stmts = Vec::new();
        while !self.at_any(&[TokenKind::KwJoin, TokenKind::KwJoin_any, TokenKind::KwJoin_none, TokenKind::Eof]) {
            stmts.push(self.parse_statement());
        }
        let join_type = match self.current_kind() {
            TokenKind::KwJoin_any => { self.bump(); JoinType::JoinAny }
            TokenKind::KwJoin_none => { self.bump(); JoinType::JoinNone }
            _ => { self.expect(TokenKind::KwJoin); JoinType::Join }
        };
        let _ = self.parse_end_label();
        Statement::new(StatementKind::ParBlock { name, join_type, stmts }, self.span_from(start))
    }

    fn parse_if_or_case(&mut self) -> Statement {
        let up = self.parse_unique_priority();
        if self.at(TokenKind::KwIf) {
            self.parse_if_with_priority(up)
        } else if self.at_any(&[TokenKind::KwCase, TokenKind::KwCasex, TokenKind::KwCasez]) {
            self.parse_case_with_priority(up)
        } else {
            self.parse_if_with_priority(up)
        }
    }

    fn parse_unique_priority(&mut self) -> Option<UniquePriority> {
        match self.current_kind() {
            TokenKind::KwUnique => { self.bump(); Some(UniquePriority::Unique) }
            TokenKind::KwUnique0 => { self.bump(); Some(UniquePriority::Unique0) }
            TokenKind::KwPriority => { self.bump(); Some(UniquePriority::Priority) }
            _ => None,
        }
    }

    fn parse_if_with_priority(&mut self, up: Option<UniquePriority>) -> Statement {
        let start = self.current().span.start;
        self.expect(TokenKind::KwIf);
        self.expect(TokenKind::LParen);
        let condition = self.parse_expression();
        self.expect(TokenKind::RParen);
        let then_stmt = self.parse_statement();
        let else_stmt = if self.eat(TokenKind::KwElse).is_some() {
            Some(Box::new(self.parse_statement()))
        } else { None };
        Statement::new(StatementKind::If {
            condition, then_stmt: Box::new(then_stmt), else_stmt,
            unique_priority: up,
        }, self.span_from(start))
    }

    fn parse_case_statement(&mut self) -> Statement {
        self.parse_case_with_priority(None)
    }

    fn parse_case_with_priority(&mut self, up: Option<UniquePriority>) -> Statement {
        let start = self.current().span.start;
        let kind = match self.bump().kind {
            TokenKind::KwCasex => CaseKind::Casex,
            TokenKind::KwCasez => CaseKind::Casez,
            _ => CaseKind::Case,
        };
        self.expect(TokenKind::LParen);
        let expr = self.parse_expression();
        self.expect(TokenKind::RParen);
        // Check for "inside" keyword
        let kind = if kind == CaseKind::Case && self.eat(TokenKind::KwInside).is_some() {
            CaseKind::CaseInside
        } else { kind };

        let mut items = Vec::new();
        while !self.at(TokenKind::KwEndcase) && !self.at(TokenKind::Eof) {
            let istart = self.current().span.start;
            if self.eat(TokenKind::KwDefault).is_some() {
                self.eat(TokenKind::Colon);
                let stmt = self.parse_statement();
                items.push(CaseItem { patterns: Vec::new(), is_default: true, stmt, span: self.span_from(istart) });
            } else {
                let mut patterns = Vec::new();
                loop {
                    patterns.push(self.parse_expression());
                    if self.eat(TokenKind::Comma).is_none() { break; }
                }
                self.expect(TokenKind::Colon);
                let stmt = self.parse_statement();
                items.push(CaseItem { patterns, is_default: false, stmt, span: self.span_from(istart) });
            }
        }
        self.expect(TokenKind::KwEndcase);
        Statement::new(StatementKind::Case {
            unique_priority: up, kind, expr, items,
        }, self.span_from(start))
    }

    fn parse_for_statement(&mut self) -> Statement {
        let start = self.current().span.start;
        self.expect(TokenKind::KwFor);
        self.expect(TokenKind::LParen);
        // Init
        let mut init = Vec::new();
        if !self.at(TokenKind::Semicolon) {
            if self.is_data_type_keyword() || (self.at(TokenKind::Identifier) && self.peek_kind() == TokenKind::Identifier) {
                let dt = self.parse_data_type();
                let name = self.parse_identifier();
                self.expect(TokenKind::Assign);
                let val = self.parse_expression();
                init.push(ForInit::VarDecl { data_type: dt, name, init: val });
            } else {
                let lv = self.parse_expression();
                self.expect(TokenKind::Assign);
                let rv = self.parse_expression();
                init.push(ForInit::Assign { lvalue: lv, rvalue: rv });
            }
        }
        self.expect(TokenKind::Semicolon);
        let condition = if !self.at(TokenKind::Semicolon) {
            Some(self.parse_expression())
        } else { None };
        self.expect(TokenKind::Semicolon);
        let mut step = Vec::new();
        if !self.at(TokenKind::RParen) {
            loop {
                // Step can be assignment (i = i + 1) or expression (i++)
                let expr = self.parse_expression();
                if self.eat(TokenKind::Assign).is_some() {
                    let rhs = self.parse_expression();
                    // Wrap as AssignOp expression (lhs = rhs)
                    step.push(Expression::new(
                        ExprKind::Binary { op: BinaryOp::Assign, left: Box::new(expr), right: Box::new(rhs) },
                        crate::ast::Span { start: 0, end: 0 },
                    ));
                } else {
                    step.push(expr);
                }
                if !self.eat(TokenKind::Comma).is_some() { break; }
            }
        }
        self.expect(TokenKind::RParen);
        let body = self.parse_statement();
        Statement::new(StatementKind::For {
            init, condition, step, body: Box::new(body),
        }, self.span_from(start))
    }

    fn parse_foreach_statement(&mut self) -> Statement {
        let start = self.current().span.start;
        self.expect(TokenKind::KwForeach);
        self.expect(TokenKind::LParen);
        
        // Array name: can be hierarchical, but NO indices yet.
        let array_hier = self.parse_hierarchical_identifier();
        let array_expr = Expression::new(ExprKind::Ident(array_hier), self.span_from(start));
        // Actually, parse_expression_prefix might be too limited.
        // Let's just parse a HierarchicalIdentifier manually or via a new helper.
        // For UVM, most are simple or pkg::name.
        
        let mut vars = Vec::new();
        self.expect(TokenKind::LBracket);
        loop {
            if self.at(TokenKind::RBracket) { break; }
            if self.at(TokenKind::Comma) {
                vars.push(None);
            } else {
                vars.push(Some(self.parse_identifier()));
            }
            if self.eat(TokenKind::Comma).is_none() { break; }
        }
        self.expect(TokenKind::RBracket);
        
        self.expect(TokenKind::RParen);
        let body = self.parse_statement();
        Statement::new(StatementKind::Foreach {
            array: array_expr, vars, body: Box::new(body),
        }, self.span_from(start))
    }

    fn parse_while_statement(&mut self) -> Statement {
        let start = self.current().span.start;
        self.expect(TokenKind::KwWhile);
        self.expect(TokenKind::LParen);
        let condition = self.parse_expression();
        self.expect(TokenKind::RParen);
        let body = self.parse_statement();
        Statement::new(StatementKind::While { condition, body: Box::new(body) }, self.span_from(start))
    }

    fn parse_do_while_statement(&mut self) -> Statement {
        let start = self.current().span.start;
        self.expect(TokenKind::KwDo);
        let body = self.parse_statement();
        self.expect(TokenKind::KwWhile);
        self.expect(TokenKind::LParen);
        let condition = self.parse_expression();
        self.expect(TokenKind::RParen);
        self.expect(TokenKind::Semicolon);
        Statement::new(StatementKind::DoWhile { body: Box::new(body), condition }, self.span_from(start))
    }

    fn parse_repeat_statement(&mut self) -> Statement {
        let start = self.current().span.start;
        self.expect(TokenKind::KwRepeat);
        self.expect(TokenKind::LParen);
        let count = self.parse_expression();
        self.expect(TokenKind::RParen);
        let body = self.parse_statement();
        Statement::new(StatementKind::Repeat { count, body: Box::new(body) }, self.span_from(start))
    }

    pub(super) fn parse_event_control(&mut self) -> EventControl {
        self.expect(TokenKind::At);
        if self.eat(TokenKind::Star).is_some() {
            return EventControl::Star;
        }
        if self.eat(TokenKind::LParen).is_some() {
            if self.eat(TokenKind::Star).is_some() {
                self.expect(TokenKind::RParen);
                return EventControl::ParenStar;
            }
            let mut events = Vec::new();
            loop {
                let estart = self.current().span.start;
                let edge = match self.current_kind() {
                    TokenKind::KwPosedge => { self.bump(); Some(Edge::Posedge) }
                    TokenKind::KwNegedge => { self.bump(); Some(Edge::Negedge) }
                    TokenKind::KwEdge => { self.bump(); Some(Edge::Edge) }
                    _ => None,
                };
                let expr = self.parse_expression();
                let iff = if self.eat(TokenKind::KwIff).is_some() {
                    Some(self.parse_expression())
                } else { None };
                events.push(EventExpr { edge, expr, iff, span: self.span_from(estart) });
                if self.eat(TokenKind::KwOr).is_some() || self.eat(TokenKind::Comma).is_some() {
                    continue;
                }
                break;
            }
            self.expect(TokenKind::RParen);
            EventControl::EventExpr(events)
        } else {
            let expr = self.parse_hierarchical_identifier_expr();
            EventControl::HierIdentifier(expr)
        }
    }

    pub(super) fn parse_assertion_statement(&mut self) -> AssertionStatement {
        let start = self.current().span.start;
        let kind = match self.bump().kind {
            TokenKind::KwAssume => AssertionKind::Assume,
            TokenKind::KwCover => AssertionKind::Cover,
            _ => AssertionKind::Assert,
        };
        self.eat(TokenKind::KwProperty); // Optional "property" keyword
        self.expect(TokenKind::LParen);
        let expr = self.parse_expression();
        self.expect(TokenKind::RParen);
        let action = if !self.at(TokenKind::Semicolon) && !self.at(TokenKind::KwElse) {
            Some(Box::new(self.parse_statement()))
        } else {
            if self.at(TokenKind::Semicolon) { self.bump(); }
            None
        };
        let else_action = if self.eat(TokenKind::KwElse).is_some() {
            Some(Box::new(self.parse_statement()))
        } else { None };
        AssertionStatement { kind, expr, action, else_action, span: self.span_from(start) }
    }
}
