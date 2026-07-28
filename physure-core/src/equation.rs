use crate::Quantity;

#[derive(Debug, Clone)]
pub struct PhyEquation {
    pub expression: String,
}

impl PhyEquation {
    pub fn parse(expr: impl Into<String>) -> Result<Self, String> {
        let expression = expr.into().trim().trim_matches('"').to_string();
        Ok(Self { expression })
    }

    pub fn call_kwargs(&self, args: &[(&str, Quantity)]) -> Result<Quantity, String> {
        let mut script = String::from("use solve from calc\neq_temp = \"");
        script.push_str(&self.expression);
        script.push_str("\"\nsolve_fn = solve(eq_temp)\nsolve_fn(");
        for (i, (k, v)) in args.iter().enumerate() {
            if i > 0 {
                script.push_str(", ");
            }
            script.push_str(&format!("{} = ({} {})", k, v.value.mean(), v.unit.display_name.as_deref().unwrap_or("")));
        }
        script.push_str(")\n");
        let results = physure_script::eval_phs(&script).map_err(|e| e.to_string())?;
        match results.into_iter().last() {
            Some(physure_script::interpreter::PhsValue::Quantity(q)) => Ok(q),
            _ => Err("Equation evaluation produced no valid Quantity result".to_string()),
        }
    }

    pub fn solve(&self, var: &str) -> Result<PhyEquation, String> {
        let mut script = String::from("use solve from calc\neq_temp = \"");
        script.push_str(&self.expression);
        script.push_str("\"\nsolve(eq_temp, \"");
        script.push_str(var);
        script.push_str("\")\n");
        let results = physure_script::eval_phs(&script).map_err(|e| e.to_string())?;
        if let Some(physure_script::interpreter::PhsValue::Equation(l, r)) = results.into_iter().last() {
            let solved_str = format!("{} = {}", l.to_phs_string(), r.to_phs_string());
            return PhyEquation::parse(solved_str);
        }
        PhyEquation::parse(format!("{} = solve({}, {})", var, self.expression, var))
    }
}
