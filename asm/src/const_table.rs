use std::collections::{HashMap, HashSet};

use crate::ast;
use crate::diagnostics::Diagnostics;

#[derive(Debug)]
pub struct ConstTable {
    const_values: HashMap<ast::Ident, ast::Immediate>,
}

impl ConstTable {
    pub fn new(prog: &ast::Program, diag: &Diagnostics, labels: &HashSet<ast::Ident>) -> Self {
        //TODO: Collect constant values and check that there are no conflicts with labels
        todo!()
    }

    /// Replaces all constant names with the immediate values that they map to
    pub fn subst(&self, stmt: ast::Stmt) -> ast::Stmt {
        //TODO: Preserve the Span of the replaced value so error messages point to the right
        // part of the code
        todo!()
    }
}
