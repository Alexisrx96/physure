use std::collections::HashMap;
use std::sync::Arc;
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity;
use physure_core::units::parser::Parser as UnitParser;
use physure_core::units::RationalUnit;

use crate::ast::{BinaryOp, Expr, Program, Statement};
use crate::resolver::{ModuleResolver, FsModuleResolver};
use crate::PhsValue;

pub struct PhsInterpreter {
    pub env: HashMap<String, PhsValue>,
    pub resolver: Arc<dyn ModuleResolver>,
}

impl Default for PhsInterpreter {
    fn default() -> Self {
        Self::new(Arc::new(FsModuleResolver))
    }
}

impl PhsInterpreter {
    pub fn new(resolver: Arc<dyn ModuleResolver>) -> Self {
        Self {
            env: HashMap::new(),
            resolver,
        }
    }

    pub fn new_default() -> Self {
        Self::default()
    }

    pub fn eval_str(&mut self, code: &str) -> PhysureResult<Vec<PhsValue>> {
        let prog = crate::parse_phs(code)?;
        let mut results = Vec::new();
        for stmt in &prog.statements {
            results.push(self.eval_statement(stmt)?);
        }
        Ok(results)
    }

    pub fn run_statement(&mut self, stmt: &Statement) -> PhysureResult<PhsValue> {
        self.eval_statement(stmt)
    }

    pub fn get_var(&self, name: &str) -> Option<&PhsValue> {
        self.env.get(name)
    }

    pub fn env(&self) -> &HashMap<String, PhsValue> {
        &self.env
    }

    pub fn get_fn_params(&self, name: &str) -> Option<Vec<String>> {
        if let Some(PhsValue::Function(f)) = self.env.get(name) {
            Some(f.params.clone())
        } else {
            None
        }
    }

    pub fn eval_program(&mut self, program: &Program) -> PhysureResult<HashMap<String, PhsValue>> {
        for stmt in &program.statements {
            self.eval_statement(stmt)?;
        }
        Ok(self.env.clone())
    }

    pub fn eval_statement(&mut self, stmt: &Statement) -> PhysureResult<PhsValue> {
        match stmt {
            Statement::Assignment(node) => {
                let val = self.eval_expr(&node.value, &self.env)?;
                self.env.insert(node.name.clone(), val.clone());
                Ok(val)
            }
            Statement::FunctionDef(node) => {
                self.env.insert(node.name.clone(), PhsValue::Function(node.clone()));
                Ok(PhsValue::None)
            }
            Statement::Expr(expr) => {
                self.eval_expr(expr, &self.env)
            }
            Statement::Import(node) => {
                let export = self.resolver.resolve(&node.path).map_err(|e| PhysureError::Generic(format!("{:?}", e)))?;
                
                match &node.specifier {
                    crate::ast::ImportSpecifier::Wildcard => {
                        for (name, expr) in export.symbols {
                            let val = self.eval_expr(&expr, &self.env)?;
                            self.env.insert(name, val);
                        }
                        for (name, func) in export.functions {
                            self.env.insert(name, PhsValue::Function(func));
                        }
                    }
                    crate::ast::ImportSpecifier::Symbols(syms) => {
                        for sym in syms {
                            if let Some(expr) = export.symbols.get(&sym.name) {
                                let val = self.eval_expr(expr, &self.env)?;
                                let target_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                                self.env.insert(target_name, val);
                            } else if let Some(func) = export.functions.get(&sym.name) {
                                let target_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                                self.env.insert(target_name, PhsValue::Function(func.clone()));
                            } else {
                                return Err(PhysureError::Generic(format!("Symbol {} not found in module {}", sym.name, node.path)));
                            }
                        }
                    }
                    crate::ast::ImportSpecifier::ModuleAlias(_alias) => {
                        return Err(PhysureError::Generic("Module aliases not yet supported by interpreter".into()));
                    }
                }
                
                Ok(PhsValue::None)
            }
            Statement::Export(_node) => {
                Ok(PhsValue::None)
            }
        }
    }

    pub fn eval_expr(&self, expr: &Expr, env: &HashMap<String, PhsValue>) -> PhysureResult<PhsValue> {
        match expr {
            Expr::Quantity(node) => {
                let mut q = Quantity::new_scalar(node.magnitude, node.uncertainty.unwrap_or(0.0), RationalUnit::dimensionless(), None, None);
                if let Some(unit_str) = &node.unit {
                    if !unit_str.is_empty() {
                        let parsed_unit = UnitParser::parse_expression(unit_str)?;
                        q = Quantity::new_scalar(node.magnitude, node.uncertainty.unwrap_or(0.0), parsed_unit, None, None);
                    }
                }
                Ok(PhsValue::Quantity(q))
            }
            Expr::Identifier(name) => {
                if let Some(val) = env.get(name) {
                    Ok(val.clone())
                } else {
                    Ok(PhsValue::String(name.clone()))
                }
            }
            Expr::BinaryOp { op, left, right } => {
                if *op == BinaryOp::Convert {
                    let q = self.eval_expr(left, env)?;
                    if let PhsValue::Quantity(q_val) = q {
                        if let Expr::Identifier(ref target_unit) = **right {
                            let reg = physure_core::UnitRegistry::build_default_si();
                            let parsed_unit = physure_core::units::parser::Parser::parse_expression_with_registry(target_unit, &reg)?;
                            let converted = q_val.convert_to(&parsed_unit)?;
                            return Ok(PhsValue::Quantity(converted));
                        } else {
                            return Ok(PhsValue::Quantity(q_val));
                        }
                    } else {
                        return Ok(q);
                    }
                }
                let l_val = self.eval_expr(left, env)?;
                let r_val = self.eval_expr(right, env)?;
                
                match (l_val, r_val) {
                    (PhsValue::Quantity(l), PhsValue::Quantity(r)) => {
                        let res = match op {
                            BinaryOp::Add => l.add(&r)?,
                            BinaryOp::Sub => l.sub(&r)?,
                            BinaryOp::Mul => l.mul(&r)?,
                            BinaryOp::Div => l.div(&r)?,
                            BinaryOp::Pow => {
                                if r.unit == RationalUnit::dimensionless() && r.value.std_dev() == 0.0 {
                                    l.pow(r.value.mean())?
                                } else {
                                    return Err(PhysureError::Generic("Exponent must be a dimensionless constant".into()));
                                }
                            }
                            BinaryOp::Convert => unreachable!(),
                        };
                        Ok(PhsValue::Quantity(res))
                    }
                    (PhsValue::Number(l), PhsValue::Number(r)) => {
                        let res = match op {
                            BinaryOp::Add => l + r,
                            BinaryOp::Sub => l - r,
                            BinaryOp::Mul => l * r,
                            BinaryOp::Div => {
                                if r == 0.0 {
                                    return Err(PhysureError::Generic("Division by zero".into()));
                                }
                                l / r
                            }
                            BinaryOp::Pow => l.powf(r),
                            BinaryOp::Convert => unreachable!(),
                        };
                        Ok(PhsValue::Number(res))
                    }
                    (PhsValue::Quantity(l), PhsValue::Number(r)) => {
                        let r_q = Quantity::new_scalar(r, 0.0, RationalUnit::dimensionless(), None, None);
                        let res = match op {
                            BinaryOp::Add => l.add(&r_q)?,
                            BinaryOp::Sub => l.sub(&r_q)?,
                            BinaryOp::Mul => l.mul(&r_q)?,
                            BinaryOp::Div => l.div(&r_q)?,
                            BinaryOp::Pow => l.pow(r)?,
                            BinaryOp::Convert => unreachable!(),
                        };
                        Ok(PhsValue::Quantity(res))
                    }
                    (PhsValue::Number(l), PhsValue::Quantity(r)) => {
                        let l_q = Quantity::new_scalar(l, 0.0, RationalUnit::dimensionless(), None, None);
                        let res = match op {
                            BinaryOp::Add => l_q.add(&r)?,
                            BinaryOp::Sub => l_q.sub(&r)?,
                            BinaryOp::Mul => l_q.mul(&r)?,
                            BinaryOp::Div => l_q.div(&r)?,
                            BinaryOp::Pow => return Err(PhysureError::Generic("Quantity exponent not supported".into())),
                            BinaryOp::Convert => unreachable!(),
                        };
                        Ok(PhsValue::Quantity(res))
                    }
                    _ => Err(PhysureError::Generic("Invalid operand types for binary operation".into())),
                }
            }
            Expr::FunctionCall { name, args } => {
                let mut arg_vals = Vec::new();
                for arg in args {
                    arg_vals.push(self.eval_expr(arg, env)?);
                }

                if let Some(val) = crate::builtins::eval_builtin(name, &arg_vals, self)? {
                    return Ok(val);
                }

                if let Some(PhsValue::Function(func)) = env.get(name) {
                    if func.params.len() != args.len() {
                        return Err(PhysureError::Generic(format!("Function {} expects {} args, got {}", name, func.params.len(), args.len())));
                    }
                    let mut local_env = env.clone();
                    for (param_name, arg_val) in func.params.iter().zip(arg_vals.into_iter()) {
                        local_env.insert(param_name.clone(), arg_val);
                    }
                    self.eval_expr(&func.body, &local_env)
                } else {
                    Err(PhysureError::Generic(format!("Undefined function '{}'", name)))
                }
            }
        }
    }
}

pub fn eval_phs(input: &str) -> PhysureResult<Vec<PhsValue>> {
    let program = crate::parser::parse_phs(input)?;
    let mut interp = PhsInterpreter::default();
    
    let mut results = Vec::new();
    for stmt in &program.statements {
        let val = interp.eval_statement(stmt)?;
        if val != PhsValue::None {
            results.push(val);
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::resolver::{MemoryModuleResolver, ModuleExport};
    
    #[test]
    fn test_kinetic_energy() {
        let mut interp = PhsInterpreter::default();
        
        let statements = vec![
            Statement::FunctionDef(FunctionDefNode {
                name: "kinetic_energy".to_string(),
                params: vec!["m".to_string(), "v".to_string()],
                body: Expr::BinaryOp {
                    op: BinaryOp::Mul,
                    left: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Mul,
                        left: Box::new(Expr::Quantity(QuantityNode {
                            magnitude: 0.5,
                            uncertainty: None,
                            unit: None,
                        })),
                        right: Box::new(Expr::Identifier("m".to_string())),
                    }),
                    right: Box::new(Expr::BinaryOp {
                        op: BinaryOp::Pow,
                        left: Box::new(Expr::Identifier("v".to_string())),
                        right: Box::new(Expr::Quantity(QuantityNode {
                            magnitude: 2.0,
                            uncertainty: None,
                            unit: None,
                        })),
                    })
                }
            }),
            Statement::Assignment(AssignmentNode {
                name: "m".to_string(),
                value: Expr::Quantity(QuantityNode {
                    magnitude: 10.0,
                    uncertainty: None,
                    unit: Some("kg".to_string()),
                }),
            }),
            Statement::Assignment(AssignmentNode {
                name: "v".to_string(),
                value: Expr::Quantity(QuantityNode {
                    magnitude: 2.0,
                    uncertainty: None,
                    unit: Some("m/s".to_string()),
                }),
            }),
            Statement::Assignment(AssignmentNode {
                name: "E".to_string(),
                value: Expr::FunctionCall {
                    name: "kinetic_energy".to_string(),
                    args: vec![
                        Expr::Identifier("m".to_string()),
                        Expr::Identifier("v".to_string()),
                    ],
                },
            }),
        ];
        
        let program = Program { statements };
        let env = interp.eval_program(&program).unwrap();
        
        let e_val = env.get("E").unwrap();
        if let PhsValue::Quantity(q) = e_val {
            assert_eq!(q.value.mean(), 20.0);
            
            // Check that it's equivalent to 20 J
            let parsed_j = UnitParser::parse_expression("J").unwrap();
            assert!(q.unit.same_dimensions(&parsed_j));
        } else {
            panic!("Expected quantity");
        }
    }
    
    #[test]
    fn test_uncertainty_propagation() {
        let mut interp = PhsInterpreter::default();
        let program = Program {
            statements: vec![
                Statement::Assignment(AssignmentNode {
                    name: "m".to_string(),
                    value: Expr::Quantity(QuantityNode {
                        magnitude: 75.0,
                        uncertainty: Some(0.5),
                        unit: Some("kg".to_string()),
                    }),
                }),
            ],
        };
        let env = interp.eval_program(&program).unwrap();
        let m_val = env.get("m").unwrap();
        if let PhsValue::Quantity(q) = m_val {
            assert_eq!(q.value.mean(), 75.0);
            assert_eq!(q.value.std_dev(), 0.5);
            assert_eq!(q.unit.__repr__(), "kg");
        } else {
            panic!("Expected quantity");
        }
    }
    
    #[test]
    fn test_virtual_module_import() {
        let mut resolver = MemoryModuleResolver::new();
        let mut export = ModuleExport {
            symbols: HashMap::new(),
            functions: HashMap::new(),
        };
        export.symbols.insert("G".to_string(), Expr::Quantity(QuantityNode {
            magnitude: 6.674e-11,
            uncertainty: None,
            unit: Some("m^3 / (kg * s^2)".to_string()),
        }));
        resolver.add_module("constants".to_string(), export);
        
        let mut interp = PhsInterpreter::new(Arc::new(resolver));
        let program = Program {
            statements: vec![
                Statement::Import(ImportNode {
                    path: "constants".to_string(),
                    specifier: ImportSpecifier::Wildcard,
                })
            ],
        };
        let env = interp.eval_program(&program).unwrap();
        let g_val = env.get("G").unwrap();
        if let PhsValue::Quantity(q) = g_val {
            assert_eq!(q.value.mean(), 6.674e-11);
        } else {
            panic!("Expected quantity");
        }
    }
}
