use super::{FromConjureModel, SolverError};
use crate::Solver;
use std::collections::HashMap;
use thiserror::Error;

use crate::ast::{
    Domain as ConjureDomain, Expression as ConjureExpression, Model as ConjureModel,
    Name as ConjureName,
};

const SOLVER: Solver = Solver::KissSAT;

struct CNF {
    pub clauses: Vec<Vec<i32>>,
    variables: HashMap<ConjureName, i32>,
    next_ind: i32,
}

/**
 * Error type for CNF adapter
 */
#[derive(Error, Debug)]
pub enum CNFError {
    #[error("Variable with name `{0}` not found")]
    VariableNameNotFound(ConjureName),

    #[error("Clause with index `{0}` not found")]
    ClauseIndexNotFound(i32),

    #[error("Unexpected Expression `{0}` inside Not(). Only Not(Reference) allowed!")]
    UnexpectedExpressionInsideNot(ConjureExpression),

    #[error(
        "Unexpected Expression `{0}` found. Only Reference, Not(Reference) and Or(...) allowed!"
    )]
    UnexpectedExpression(ConjureExpression),

    #[error("Unexpected nested And: {0}")]
    NestedAnd(ConjureExpression),
}

impl CNF {
    pub fn new() -> CNF {
        CNF {
            clauses: Vec::new(),
            variables: HashMap::new(),
            next_ind: 1,
        }
    }

    /**
     * Get all the Conjure variables in the CNF
     */
    pub fn get_variables(&self) -> Vec<&ConjureName> {
        let mut ans: Vec<&ConjureName> = Vec::new();

        for key in self.variables.keys() {
            ans.push(key);
        }

        ans
    }

    /**
     * Get the index of a Conjure variable
     */
    pub fn get_index(&self, var: &ConjureName) -> Option<i32> {
        return self.variables.get(var).copied();
    }

    /**
     * Get the Conjure variable from its index
     */
    pub fn get_name(&self, ind: i32) -> Option<&ConjureName> {
        for key in self.variables.keys() {
            let idx = self.get_index(key)?;
            if idx == ind {
                return Some(key);
            }
        }

        None
    }

    /**
     * Add a new Conjure variable to the CNF
     */
    pub fn add_variable(&mut self, var: &ConjureName) {
        self.variables.insert(var.clone(), self.next_ind);
        self.next_ind += 1;
    }

    /**
     * Check if a Conjure variable or index is present in the CNF
     */
    pub fn has_variable<T: HasVariable>(&self, value: T) -> bool {
        value.has_variable(self)
    }

    /**
     * Add a new clause to the CNF. Must be a vector of indices in CNF format
     */
    pub fn add_clause(&mut self, vec: &Vec<i32>) -> Result<(), CNFError> {
        for idx in vec {
            if !self.has_variable(idx.abs()) {
                return Err(CNFError::ClauseIndexNotFound(*idx));
            }
        }
        self.clauses.push(vec.clone());
        Ok(())
    }

    /**
     * Add a new Conjure expression to the CNF. Must be a logical expression in CNF form
     */
    pub fn add_expression(&mut self, expr: &ConjureExpression) -> Result<(), CNFError> {
        for row in self.handle_expression(expr)? {
            self.add_clause(&row)?;
        }
        Ok(())
    }

    /**
     * Convert the CNF to a Conjure expression
     */
    pub fn as_expression(&self) -> Result<ConjureExpression, CNFError> {
        let mut expr_clauses: Vec<ConjureExpression> = Vec::new();

        for clause in &self.clauses {
            expr_clauses.push(self.clause_to_expression(clause)?);
        }

        Ok(ConjureExpression::And(expr_clauses))
    }

    /**
     * Convert a single clause to a Conjure expression
     */
    fn clause_to_expression(&self, clause: &Vec<i32>) -> Result<ConjureExpression, CNFError> {
        let mut ans: Vec<ConjureExpression> = Vec::new();

        for idx in clause {
            match self.get_name(idx.abs()) {
                None => return Err(CNFError::ClauseIndexNotFound(*idx)),
                Some(name) => {
                    if *idx > 0 {
                        ans.push(ConjureExpression::Reference(name.clone()))
                    } else {
                        let expression: ConjureExpression =
                            ConjureExpression::Reference(name.clone());
                        ans.push(ConjureExpression::Not(Box::from(expression)))
                    }
                }
            }
        }

        Ok(ConjureExpression::Or(ans))
    }

    /**
     * Get the index for a Conjure Reference or return an error
     * @see get_index
     * @see ConjureExpression::Reference
     */
    fn get_reference_index(&self, name: &ConjureName) -> Result<i32, CNFError> {
        match self.get_index(name) {
            None => Err(CNFError::VariableNameNotFound(name.clone())),
            Some(ind) => Ok(ind),
        }
    }

    /**
     * Convert the contents of a single Reference to a row of the CNF format
     * @see get_reference_index
     * @see ConjureExpression::Reference
     */
    fn handle_reference(&self, name: &ConjureName) -> Result<Vec<i32>, CNFError> {
        Ok(vec![self.get_reference_index(name)?])
    }

    /**
     * Convert the contents of a single Not() to CNF
     */
    fn handle_not(&self, expr_box: &Box<ConjureExpression>) -> Result<Vec<i32>, CNFError> {
        let expr = expr_box.as_ref();
        match expr {
            // Expression inside the Not()
            ConjureExpression::Reference(name) => Ok(vec![-self.get_reference_index(name)?]),
            _ => Err(CNFError::UnexpectedExpressionInsideNot(expr.clone())),
        }
    }

    /**
     * Convert the contents of a single Or() to a row of the CNF format
     */
    fn handle_or(&self, expressions: &Vec<ConjureExpression>) -> Result<Vec<i32>, CNFError> {
        let mut ans: Vec<i32> = Vec::new();

        for expr in expressions {
            let ret = self.handle_flat_expression(expr)?;
            for ind in ret {
                ans.push(ind);
            }
        }

        Ok(ans)
    }

    /**
     * Convert a single Reference, Not or Or into a clause of the CNF format
     */
    fn handle_flat_expression(&self, expression: &ConjureExpression) -> Result<Vec<i32>, CNFError> {
        match expression {
            ConjureExpression::Reference(name) => self.handle_reference(name),
            ConjureExpression::Not(var_box) => self.handle_not(var_box),
            ConjureExpression::Or(expressions) => self.handle_or(expressions),
            _ => Err(CNFError::UnexpectedExpression(expression.clone())),
        }
    }

    /**
     * Convert a single And() into a vector of clauses in the CNF format
     */
    fn handle_and(&self, expressions: &Vec<ConjureExpression>) -> Result<Vec<Vec<i32>>, CNFError> {
        let mut ans: Vec<Vec<i32>> = Vec::new();

        for expression in expressions {
            match expression {
                ConjureExpression::And(_expressions) => {
                    return Err(CNFError::NestedAnd(expression.clone()));
                }
                _ => {
                    ans.push(self.handle_flat_expression(expression)?);
                }
            }
        }

        Ok(ans)
    }

    /**
     * Convert a single Conjure expression into a vector of clauses of the CNF format
     */
    fn handle_expression(&self, expression: &ConjureExpression) -> Result<Vec<Vec<i32>>, CNFError> {
        match expression {
            ConjureExpression::And(expressions) => self.handle_and(expressions),
            _ => Ok(vec![self.handle_flat_expression(expression)?]),
        }
    }
}

/**
 * Helper trait for checking if a variable is present in the CNF polymorphically (i32 or ConjureName)
 */
trait HasVariable {
    fn has_variable(self, cnf: &CNF) -> bool;
}

impl HasVariable for i32 {
    fn has_variable(self, cnf: &CNF) -> bool {
        return cnf.get_name(self).is_some();
    }
}

impl HasVariable for &ConjureName {
    fn has_variable(self, cnf: &CNF) -> bool {
        cnf.get_index(self).is_some()
    }
}

/**
* Expects Model to be in the Conjunctive Normal Form:
* - All variables must be boolean
* - Expressions must be Reference, Not(Reference), or Or(Reference1, Not(Reference2), ...)
* - The top level And() may contain nested Or()s. Any other nested expressions are not allowed.
*/
impl FromConjureModel for CNF {
    /**
     * Convert a Conjure model to a CNF
     */
    fn from_conjure(conjure_model: ConjureModel) -> Result<Self, SolverError> {
        let mut ans: CNF = CNF::new();

        for var in conjure_model.variables.keys() {
            // Check that domain has the correct type
            let decision_var = conjure_model.variables.get(var).unwrap();
            if decision_var.domain != ConjureDomain::BoolDomain {
                return Err(SolverError::NotSupported(
                    SOLVER,
                    format!("variable {:?} is not BoolDomain", decision_var),
                ));
            }

            ans.add_variable(var);
        }

        for expr in conjure_model.get_constraints_vec() {
            match ans.add_expression(&expr) {
                Ok(_) => {}
                Err(error) => {
                    let message = format!("{:?}", error);
                    return Err(SolverError::NotSupported(SOLVER, message));
                }
            }
        }

        Ok(ans)
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::Domain::{BoolDomain, IntDomain};
    use crate::ast::Expression::{And, Not, Or, Reference};
    use crate::ast::{DecisionVariable, Model};
    use crate::ast::{Expression, Name};
    use crate::solvers::kissat::CNF;
    use crate::solvers::{FromConjureModel, SolverError};
    use crate::utils::{assert_eq_any_order, if_ok};

    #[test]
    fn test_single_var() {
        // x -> [[1]]

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_constraint(Reference(x.clone()));

        let res: Result<CNF, SolverError> = CNF::from_conjure(model);
        assert!(res.is_ok());

        let cnf = res.unwrap();

        assert_eq!(cnf.get_index(&x), Some(1));
        assert!(cnf.get_name(1).is_some());
        assert_eq!(cnf.get_name(1).unwrap(), &x);

        assert_eq!(cnf.clauses, vec![vec![1]]);
    }

    #[test]
    fn test_single_not() {
        // Not(x) -> [[-1]]

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_constraint(Not(Box::from(Reference(x.clone()))));

        let cnf: CNF = CNF::from_conjure(model).unwrap();
        assert_eq!(cnf.get_index(&x), Some(1));
        assert_eq!(cnf.clauses, vec![vec![-1]]);

        assert_eq!(
            if_ok(cnf.as_expression()),
            And(vec![Or(vec![Not(Box::from(Reference(x.clone())))])])
        )
    }

    #[test]
    fn test_single_or() {
        // Or(x, y) -> [[1, 2]]

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        let y: Name = Name::UserName(String::from('y'));

        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_variable(y.clone(), DecisionVariable { domain: BoolDomain });

        model.add_constraint(Or(vec![Reference(x.clone()), Reference(y.clone())]));

        let cnf: CNF = CNF::from_conjure(model).unwrap();

        let xi = cnf.get_index(&x).unwrap();
        let yi = cnf.get_index(&y).unwrap();
        assert_eq_any_order(&cnf.clauses, &vec![vec![xi, yi]]);

        assert_eq!(
            if_ok(cnf.as_expression()),
            And(vec![Or(vec![Reference(x.clone()), Reference(y.clone())])])
        )
    }

    #[test]
    fn test_or_not() {
        // Or(x, Not(y)) -> [[1, -2]]

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        let y: Name = Name::UserName(String::from('y'));

        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_variable(y.clone(), DecisionVariable { domain: BoolDomain });

        model.add_constraint(Or(vec![
            Reference(x.clone()),
            Not(Box::from(Reference(y.clone()))),
        ]));

        let cnf: CNF = CNF::from_conjure(model).unwrap();

        let xi = cnf.get_index(&x).unwrap();
        let yi = cnf.get_index(&y).unwrap();
        assert_eq_any_order(&cnf.clauses, &vec![vec![xi, -yi]]);

        assert_eq!(
            if_ok(cnf.as_expression()),
            And(vec![Or(vec![
                Reference(x.clone()),
                Not(Box::from(Reference(y.clone())))
            ])])
        )
    }

    #[test]
    fn test_multiple() {
        // [x, y] - equivalent to And(x, y) -> [[1], [2]]

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        let y: Name = Name::UserName(String::from('y'));

        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_variable(y.clone(), DecisionVariable { domain: BoolDomain });

        model.add_constraint(Reference(x.clone()));
        model.add_constraint(Reference(y.clone()));

        let cnf: CNF = CNF::from_conjure(model).unwrap();

        let xi = cnf.get_index(&x).unwrap();
        let yi = cnf.get_index(&y).unwrap();
        assert_eq_any_order(&cnf.clauses, &vec![vec![xi], vec![yi]]);

        assert_eq!(
            if_ok(cnf.as_expression()),
            And(vec![
                Or(vec![Reference(x.clone())]),
                Or(vec![Reference(y.clone())])
            ])
        )
    }

    #[test]
    fn test_and() {
        // And(x, y) -> [[1], [2]]

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        let y: Name = Name::UserName(String::from('y'));

        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_variable(y.clone(), DecisionVariable { domain: BoolDomain });

        model.add_constraint(And(vec![Reference(x.clone()), Reference(y.clone())]));

        let cnf: CNF = CNF::from_conjure(model).unwrap();

        let xi = cnf.get_index(&x).unwrap();
        let yi = cnf.get_index(&y).unwrap();
        assert_eq_any_order(&cnf.clauses, &vec![vec![xi], vec![yi]]);

        assert_eq!(
            if_ok(cnf.as_expression()),
            And(vec![
                Or(vec![Reference(x.clone())]),
                Or(vec![Reference(y.clone())])
            ])
        )
    }

    #[test]
    fn test_nested_ors() {
        // Or(x, Or(y, z)) -> [[1, 2, 3]]

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        let y: Name = Name::UserName(String::from('y'));
        let z: Name = Name::UserName(String::from('z'));

        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_variable(y.clone(), DecisionVariable { domain: BoolDomain });
        model.add_variable(z.clone(), DecisionVariable { domain: BoolDomain });

        model.add_constraint(Or(vec![
            Reference(x.clone()),
            Or(vec![Reference(y.clone()), Reference(z.clone())]),
        ]));

        let cnf: CNF = CNF::from_conjure(model).unwrap();

        let xi = cnf.get_index(&x).unwrap();
        let yi = cnf.get_index(&y).unwrap();
        let zi = cnf.get_index(&z).unwrap();
        assert_eq_any_order(&cnf.clauses, &vec![vec![xi, yi, zi]]);

        assert_eq!(
            if_ok(cnf.as_expression()),
            And(vec![Or(vec![
                Reference(x.clone()),
                Reference(y.clone()),
                Reference(z.clone())
            ])])
        )
    }

    #[test]
    fn test_int() {
        // y is an IntDomain - only booleans should be allowed

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        let y: Name = Name::UserName(String::from('y'));

        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_variable(
            y.clone(),
            DecisionVariable {
                domain: IntDomain(vec![]),
            },
        );

        model.add_constraint(Reference(x.clone()));
        model.add_constraint(Reference(y.clone()));

        let cnf: Result<CNF, SolverError> = CNF::from_conjure(model);
        assert!(cnf.is_err());
    }

    #[test]
    fn test_eq() {
        // Eq(x, y) - this operation is not allowed

        let mut model: Model = Model::new();

        let x: Name = Name::UserName(String::from('x'));
        let y: Name = Name::UserName(String::from('y'));

        model.add_variable(x.clone(), DecisionVariable { domain: BoolDomain });
        model.add_variable(y.clone(), DecisionVariable { domain: BoolDomain });

        model.add_constraint(Expression::Eq(
            Box::from(Reference(x.clone())),
            Box::from(Reference(y.clone())),
        ));

        let cnf: Result<CNF, SolverError> = CNF::from_conjure(model);
        assert!(cnf.is_err());
    }
}
