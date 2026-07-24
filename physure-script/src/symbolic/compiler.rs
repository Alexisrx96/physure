use physure_core::error::{PhysureError, PhysureResult};
use super::ast::Node;

#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    PushNumber(f64),
    PushVar(usize),
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Sin,
    Cos,
    Ln,
    Exp,
}

#[derive(Clone, Debug)]
pub struct CompiledExpr {
    pub instructions: Vec<Instruction>,
    pub var_names: Vec<String>,
}

impl CompiledExpr {
    pub fn compile(node: &Node) -> PhysureResult<Self> {
        let mut var_names = Vec::new();
        let mut instructions = Vec::new();
        Self::compile_node(node, &mut var_names, &mut instructions)?;
        Ok(CompiledExpr { instructions, var_names })
    }

    fn compile_node(node: &Node, vars: &mut Vec<String>, insts: &mut Vec<Instruction>) -> PhysureResult<()> {
        match node {
            Node::Number(n) => insts.push(Instruction::PushNumber(*n)),
            Node::Symbol(s) | Node::Quantity(s, _) => {
                let idx = match vars.iter().position(|v| v == s) {
                    Some(i) => i,
                    None => {
                        vars.push(s.clone());
                        vars.len() - 1
                    }
                };
                insts.push(Instruction::PushVar(idx));
            }
            Node::Add(terms) => {
                if terms.is_empty() {
                    insts.push(Instruction::PushNumber(0.0));
                } else {
                    Self::compile_node(&terms[0], vars, insts)?;
                    for term in &terms[1..] {
                        Self::compile_node(term, vars, insts)?;
                        insts.push(Instruction::Add);
                    }
                }
            }
            Node::Sub(a, b) => {
                Self::compile_node(a, vars, insts)?;
                Self::compile_node(b, vars, insts)?;
                insts.push(Instruction::Sub);
            }
            Node::Mul(factors) => {
                if factors.is_empty() {
                    insts.push(Instruction::PushNumber(1.0));
                } else {
                    Self::compile_node(&factors[0], vars, insts)?;
                    for factor in &factors[1..] {
                        Self::compile_node(factor, vars, insts)?;
                        insts.push(Instruction::Mul);
                    }
                }
            }
            Node::Div(a, b) => {
                Self::compile_node(a, vars, insts)?;
                Self::compile_node(b, vars, insts)?;
                insts.push(Instruction::Div);
            }
            Node::Pow(a, b) => {
                Self::compile_node(a, vars, insts)?;
                Self::compile_node(b, vars, insts)?;
                insts.push(Instruction::Pow);
            }
            Node::Sin(u) => {
                Self::compile_node(u, vars, insts)?;
                insts.push(Instruction::Sin);
            }
            Node::Cos(u) => {
                Self::compile_node(u, vars, insts)?;
                insts.push(Instruction::Cos);
            }
            Node::Ln(u) => {
                Self::compile_node(u, vars, insts)?;
                insts.push(Instruction::Ln);
            }
            Node::Exp(u) => {
                Self::compile_node(u, vars, insts)?;
                insts.push(Instruction::Exp);
            }
        }
        Ok(())
    }

    pub fn eval(&self, inputs: &[f64]) -> PhysureResult<f64> {
        if inputs.len() < self.var_names.len() {
            return Err(PhysureError::Generic(format!(
                "Expected {} inputs, got {}", self.var_names.len(), inputs.len()
            )));
        }
        let mut stack = Vec::with_capacity(16);
        for inst in &self.instructions {
            match inst {
                Instruction::PushNumber(n) => stack.push(*n),
                Instruction::PushVar(idx) => stack.push(inputs[*idx]),
                Instruction::Add => {
                    let b = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    stack.push(a + b);
                }
                Instruction::Sub => {
                    let b = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    stack.push(a - b);
                }
                Instruction::Mul => {
                    let b = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    stack.push(a * b);
                }
                Instruction::Div => {
                    let b = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    if b == 0.0 { return Err(PhysureError::DivisionByZero("Division by zero in eval".into())); }
                    stack.push(a / b);
                }
                Instruction::Pow => {
                    let b = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    stack.push(a.powf(b));
                }
                Instruction::Sin => {
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    stack.push(a.sin());
                }
                Instruction::Cos => {
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    stack.push(a.cos());
                }
                Instruction::Ln => {
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    stack.push(a.ln());
                }
                Instruction::Exp => {
                    let a = stack.pop().ok_or_else(|| PhysureError::Generic("Stack underflow".into()))?;
                    stack.push(a.exp());
                }
            }
        }
        stack.pop().ok_or_else(|| PhysureError::Generic("Empty evaluation stack".into()))
    }
}
