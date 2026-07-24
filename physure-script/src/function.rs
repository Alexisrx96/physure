use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity;
use super::interpreter::PhsInterpreter;
use super::value::PhsValue;

/// Represents a stateful, calculus-enabled physical function in Rust.
#[derive(Clone)]
pub struct PhyFunction {
    interpreter: PhsInterpreter,
    name: String,
    body: String,
}

impl PhyFunction {
    /// Creates and registers a new PhyFunction in the interpreter.
    pub fn new(interpreter: PhsInterpreter, name: impl Into<String>, body: impl Into<String>) -> PhysureResult<Self> {
        let name_str = name.into();
        let body_str = body.into();
        
        let mut interp = interpreter;
        
        // Parse and register the function in the interpreter
        let statements = crate::parse_phs(&body_str)?;
        if statements.is_empty() {
            return Err(PhysureError::Generic("Empty function body".into()));
        }
        interp.run_statement(&statements[0])?;
        
        Ok(Self {
            interpreter: interp,
            name: name_str,
            body: body_str,
        })
    }
    
    /// Gets the function's name.
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Gets the function's body.
    pub fn body(&self) -> &str {
        &self.body
    }
    
    /// Gets a reference to the internal interpreter.
    pub fn interpreter(&self) -> &PhsInterpreter {
        &self.interpreter
    }
    
    /// Gets the list of parameter names for this function.
    pub fn get_params(&self) -> Vec<String> {
        self.interpreter.get_fn_params(&self.name).unwrap_or_default()
    }
    
    /// Calls the function with quantity arguments, returning the evaluated Quantity.
    pub fn call(&mut self, args: &[Quantity]) -> PhysureResult<Quantity> {
        let mut formatted_args = Vec::new();
        for arg in args {
            // format quantity: magnitude and unit representation in ASCII (avoiding unicode)
            let mut unit_str = arg.unit.__repr__();
            // Clean unicode symbols for the PHS engine lexer
            unit_str = unit_str.replace("·", " * ");
            unit_str = unit_str.replace("⁻", "^-");
            unit_str = unit_str.replace("⁰", "^0");
            unit_str = unit_str.replace("¹", "^1");
            unit_str = unit_str.replace("²", "^2");
            unit_str = unit_str.replace("³", "^3");
            unit_str = unit_str.replace("⁴", "^4");
            unit_str = unit_str.replace("⁵", "^5");
            unit_str = unit_str.replace("⁶", "^6");
            unit_str = unit_str.replace("⁷", "^7");
            unit_str = unit_str.replace("⁸", "^8");
            unit_str = unit_str.replace("⁹", "^9");
            unit_str = unit_str.replace("^-^-", "^-");
            unit_str = unit_str.replace("^^", "^");
            
            // Format to magnitude and unit
            formatted_args.push(format!("{} {}", arg.value.mean(), unit_str));
        }
        
        let call_str = format!("{}({})", self.name, formatted_args.join(", "));
        let statements = crate::parse_phs(&call_str)?;
        if statements.is_empty() {
            return Err(PhysureError::Generic("Failed to parse call".into()));
        }
        
        let res = self.interpreter.run_statement(&statements[0])?;
        match res {
            PhsValue::Quantity(q) => Ok(q),
            PhsValue::Number(n) => Ok(Quantity::new_scalar(n, 0.0, physure_core::units::RationalUnit::dimensionless(), None, None)),
            _ => Err(PhysureError::Generic(format!("Call did not return a quantity: {:?}", res))),
        }
    }
    
    /// Returns the symbolic derivative of this function with respect to `var` as a new PhyFunction.
    pub fn deriv(&self, var: &str) -> PhysureResult<Self> {
        let params = self.get_params();
        if params.is_empty() {
            return Err(PhysureError::Generic("Cannot differentiate a function with no parameters".into()));
        }
        
        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        
        let mut interp = self.interpreter.clone();
        
        let deriv_expr = format!("deriv(\"{}\", \"{}\")", call_expr, var);
        let statements = crate::parse_phs(&deriv_expr)?;
        if statements.is_empty() {
            return Err(PhysureError::Generic("Failed to parse deriv expression".into()));
        }
        
        let res = interp.run_statement(&statements[0])?;
        let deriv_result = match res {
            PhsValue::String(s) => s,
            _ => return Err(PhysureError::Generic("deriv did not return a string expression".into())),
        };
        
        let new_name = format!("d_{}_d_{}", self.name, var);
        let new_body = format!("{}({}) = {}", new_name, params_joined, deriv_result);
        
        Self::new(interp, new_name, new_body)
    }
    
    /// Returns the symbolic integral of this function with respect to `var` as a new PhyFunction.
    pub fn integral(&self, var: &str) -> PhysureResult<Self> {
        let params = self.get_params();
        if params.is_empty() {
            return Err(PhysureError::Generic("Cannot integrate a function with no parameters".into()));
        }
        
        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        
        let mut interp = self.interpreter.clone();
        
        let integral_expr = format!("integral(\"{}\", \"{}\")", call_expr, var);
        let statements = crate::parse_phs(&integral_expr)?;
        if statements.is_empty() {
            return Err(PhysureError::Generic("Failed to parse integral expression".into()));
        }
        
        let res = interp.run_statement(&statements[0])?;
        let integral_result = match res {
            PhsValue::String(s) => s,
            _ => return Err(PhysureError::Generic("integral did not return a string expression".into())),
        };
        
        let new_name = format!("i_{}_d_{}", self.name, var);
        let new_body = format!("{}({}) = {}", new_name, params_joined, integral_result);
        
        Self::new(interp, new_name, new_body)
    }
    
    /// Solves this function symbolically for `var`, returning a new PhyFunction with signature `(target, ...other_params)`.
    pub fn solve(&self, var: &str) -> PhysureResult<Self> {
        let params = self.get_params();
        if params.is_empty() {
            return Err(PhysureError::Generic("Cannot solve a function with no parameters".into()));
        }
        
        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        
        let mut interp = self.interpreter.clone();
        
        let solve_expr = format!("solve(\"{} = target\", \"{}\")", call_expr, var);
        let statements = crate::parse_phs(&solve_expr)?;
        if statements.is_empty() {
            return Err(PhysureError::Generic("Failed to parse solve expression".into()));
        }
        
        let res = interp.run_statement(&statements[0])?;
        let solve_result = match res {
            PhsValue::String(s) => s,
            _ => return Err(PhysureError::Generic("solve did not return a string expression".into())),
        };
        
        let new_name = format!("solve_{}_for_{}", self.name, var);
        
        // Remove the target variable from parameters, and add "target" at the front
        let other_params: Vec<String> = params.into_iter().filter(|p| p != var).collect();
        let mut new_params = vec!["target".to_string()];
        new_params.extend(other_params);
        let new_params_joined = new_params.join(", ");
        
        let new_body = format!("{}({}) = {}", new_name, new_params_joined, solve_result);
        
        Self::new(interp, new_name, new_body)
    }
    
    /// Returns a new PhyFunction representing the sum of this and other: (self + other)(x) = self(x) + other(x)
    pub fn add(&self, other: &PhyFunction) -> PhysureResult<Self> {
        self.binary_op(other, "+", "add")
    }
    
    /// Returns a new PhyFunction representing the subtraction of other from this: (self - other)(x) = self(x) - other(x)
    pub fn sub(&self, other: &PhyFunction) -> PhysureResult<Self> {
        self.binary_op(other, "-", "sub")
    }
    
    /// Returns a new PhyFunction representing the multiplication of this and other: (self * other)(x) = self(x) * other(x)
    pub fn mul(&self, other: &PhyFunction) -> PhysureResult<Self> {
        self.binary_op(other, "*", "mul")
    }
    
    /// Returns a new PhyFunction representing the division of this by other: (self / other)(x) = self(x) / other(x)
    pub fn div(&self, other: &PhyFunction) -> PhysureResult<Self> {
        self.binary_op(other, "/", "div")
    }
    
    /// Returns a new PhyFunction representing the composition of this and other: (self o other)(x) = self(other(x))
    pub fn compose(&self, other: &PhyFunction) -> PhysureResult<Self> {
        let mut interp = self.interpreter.clone();
        
        // Ensure other is registered in the cloned interpreter
        if interp.get_user_fn(&other.name).is_none() {
            let other_statements = crate::parse_phs(&other.body)?;
            if !other_statements.is_empty() {
                interp.run_statement(&other_statements[0])?;
            }
        }
        
        let params_f = self.get_params();
        let params_g = other.get_params();
        
        if params_f.is_empty() {
            return Err(PhysureError::Generic("Outer function must have at least one parameter.".into()));
        }
        
        let mut combined = params_g.clone();
        for p in &params_f[1..] {
            if !combined.contains(p) {
                combined.push(p.clone());
            }
        }
        
        let combined_params_joined = combined.join(", ");
        let call_g = format!("{}({})", other.name, params_g.join(", "));
        let mut call_f_args = vec![call_g];
        call_f_args.extend(params_f[1..].to_vec());
        
        let call_f = format!("{}({})", self.name, call_f_args.join(", "));
        
        let new_name = format!("compose_{}_{}", self.name, other.name);
        let new_body = format!("{}({}) = {}", new_name, combined_params_joined, call_f);
        
        Self::new(interp, new_name, new_body)
    }
    
    fn binary_op(&self, other: &PhyFunction, op_symbol: &str, op_name: &str) -> PhysureResult<Self> {
        let mut interp = self.interpreter.clone();
        
        // Ensure other is registered in the cloned interpreter
        if interp.get_user_fn(&other.name).is_none() {
            let other_statements = crate::parse_phs(&other.body)?;
            if !other_statements.is_empty() {
                interp.run_statement(&other_statements[0])?;
            }
        }
        
        let params1 = self.get_params();
        let params2 = other.get_params();
        
        let mut combined = params1.clone();
        for p in &params2 {
            if !combined.contains(p) {
                combined.push(p.clone());
            }
        }
        
        let combined_params_joined = combined.join(", ");
        let new_name = format!("{}_{}_{}", op_name, self.name, other.name);
        
        let new_body = format!(
            "{}({}) = {}({}) {} {}({})",
            new_name,
            combined_params_joined,
            self.name,
            params1.join(", "),
            op_symbol,
            other.name,
            params2.join(", ")
        );
        
        Self::new(interp, new_name, new_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use physure_core::quantity::Quantity;
    use physure_core::units::RationalUnit;

    #[test]
    fn test_phy_function_parity_rust() {
        let interp = PhsInterpreter::new();
        
        // 1. Register function
        let mut ke = PhyFunction::new(interp, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        assert_eq!(ke.get_params(), vec!["m".to_string(), "v".to_string()]);
        
        // 2. Call function with Quantity args
        let m_val = Quantity::new_scalar(10.0, 0.0, RationalUnit::new_from_dimensions([("kg".to_string(), (1, 1))]), None, None);
        let v_val = Quantity::new_scalar(5.0, 0.0, RationalUnit::new_from_dimensions([("m".to_string(), (1, 1)), ("s".to_string(), (-1, 1))]), None, None);
        
        let res = ke.call(&[m_val.clone(), v_val.clone()]).unwrap();
        assert_eq!(res.value.mean(), 125.0);
        assert_eq!(res.unit.__repr__(), "J");
        
        // 3. Symbolic Derivative
        let mut dke_dv = ke.deriv("v").unwrap();
        assert_eq!(dke_dv.get_params(), vec!["m".to_string(), "v".to_string()]);
        let res_deriv = dke_dv.call(&[m_val.clone(), v_val.clone()]).unwrap();
        assert_eq!(res_deriv.value.mean(), 50.0);
        assert_eq!(res_deriv.unit.__repr__().replace(" ", ""), "kg*m*s^-1");
        
        // 4. Symbolic Integration
        let mut ike_dv = ke.integral("v").unwrap();
        assert_eq!(ike_dv.get_params(), vec!["m".to_string(), "v".to_string()]);
        let res_int = ike_dv.call(&[m_val.clone(), v_val.clone()]).unwrap();
        // 1/6 * 10 * 125 = 208.33333333333334
        assert!((res_int.value.mean() - 208.33333333333334).abs() < 1e-9);
        
        // 5. Symbolic Solving
        let mut ske_dv = ke.solve("v").unwrap();
        assert_eq!(ske_dv.get_params(), vec!["target".to_string(), "m".to_string()]);
        let target_val = Quantity::new_scalar(125.0, 0.0, RationalUnit::new_from_dimensions([("J".to_string(), (1, 1))]), None, None);
        let res_solve = ske_dv.call(&[target_val, m_val]).unwrap();
        assert_eq!(res_solve.value.mean(), 5.0);
        assert_eq!(res_solve.unit.__repr__().replace(" ", ""), "m*s^-1");
        
        // 6. Function Arithmetic and Composition
        let f = PhyFunction::new(ke.interpreter().clone(), "f", "f(x) = 2 * x").unwrap();
        let g = PhyFunction::new(ke.interpreter().clone(), "g", "g(x) = 3 * x").unwrap();
        
        let mut sum_f_g = f.add(&g).unwrap();
        assert_eq!(sum_f_g.get_params(), vec!["x".to_string()]);
        let x_val = Quantity::new_scalar(2.0, 0.0, RationalUnit::dimensionless(), None, None);
        let res_sum = sum_f_g.call(&[x_val.clone()]).unwrap();
        assert_eq!(res_sum.value.mean(), 10.0);
        
        let mut comp_f_g = f.compose(&g).unwrap();
        assert_eq!(comp_f_g.get_params(), vec!["x".to_string()]);
        let res_comp = comp_f_g.call(&[x_val]).unwrap();
        assert_eq!(res_comp.value.mean(), 12.0);
    }
}
