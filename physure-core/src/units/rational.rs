use std::collections::HashMap;
use num_rational::Rational64;
use std::hash::{Hash, Hasher};
use smallvec::SmallVec;

pub type DimVec = SmallVec<[(String, (i64, i64)); 4]>;

/// A unit representation using rational exponents to avoid floating-point errors.
#[derive(Clone, Debug, Eq)]
pub struct RationalUnit {
    /// Vector of base unit names to their exponents as (numerator, denominator), maintained sorted by unit name.
    pub dimensions: DimVec,
    pub id: u64,
}

impl PartialEq for RationalUnit {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Hash for RationalUnit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl RationalUnit {
    /// Parse a rational exponent from a plain Rust (i64, i64) tuple or i64.
    pub fn parse_exponent_tuple(n: i64, den: i64) -> Option<(i64, i64)> {
        if n != 0 { Some((n, den)) } else { None }
    }

    pub fn calculate_id(dimensions: &[(String, (i64, i64))]) -> u64 {
        let mut h: u64 = 0;
        for (k, v) in dimensions {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            k.hash(&mut hasher);
            v.hash(&mut hasher);
            h ^= hasher.finish();
        }
        h
    }

    pub fn new_from_dimensions<I>(dims: I) -> Self
    where
        I: IntoIterator<Item = (String, (i64, i64))>,
    {
        let mut dimensions: DimVec = dims.into_iter().filter(|(_, (n, _))| *n != 0).collect();
        dimensions.sort_by(|a, b| a.0.cmp(&b.0));
        let id = Self::calculate_id(&dimensions);
        RationalUnit { dimensions, id }
    }

    pub fn dimensions_map(&self) -> HashMap<String, (i64, i64)> {
        self.dimensions.iter().cloned().collect()
    }

    pub fn dimensionless() -> Self {
        RationalUnit {
            dimensions: DimVec::new(),
            id: 0,
        }
    }

    pub fn base(name: &str) -> Self {
        Self::new_from_dimensions([(name.to_string(), (1, 1))])
    }

    pub fn get_exponent(&self, base: &str) -> Option<(i64, i64)> {
        self.dimensions
            .binary_search_by(|(k, _)| k.as_str().cmp(base))
            .ok()
            .map(|idx| self.dimensions[idx].1)
    }

    pub fn mul(&self, other: &Self) -> Self {
        let mut new_dims = DimVec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.dimensions.len() && j < other.dimensions.len() {
            let (k1, v1) = &self.dimensions[i];
            let (k2, v2) = &other.dimensions[j];
            match k1.cmp(k2) {
                std::cmp::Ordering::Less => {
                    new_dims.push((k1.clone(), *v1));
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    new_dims.push((k2.clone(), *v2));
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    let r1 = Rational64::new(v1.0, v1.1);
                    let r2 = Rational64::new(v2.0, v2.1);
                    let res = r1 + r2;
                    if *res.numer() != 0 {
                        new_dims.push((k1.clone(), (*res.numer(), *res.denom())));
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        while i < self.dimensions.len() {
            new_dims.push(self.dimensions[i].clone());
            i += 1;
        }
        while j < other.dimensions.len() {
            new_dims.push(other.dimensions[j].clone());
            j += 1;
        }
        let id = Self::calculate_id(&new_dims);
        RationalUnit { dimensions: new_dims, id }
    }

    pub fn div(&self, other: &Self) -> Self {
        let mut new_dims = DimVec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.dimensions.len() && j < other.dimensions.len() {
            let (k1, v1) = &self.dimensions[i];
            let (k2, v2) = &other.dimensions[j];
            match k1.cmp(k2) {
                std::cmp::Ordering::Less => {
                    new_dims.push((k1.clone(), *v1));
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    new_dims.push((k2.clone(), (-v2.0, v2.1)));
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    let r1 = Rational64::new(v1.0, v1.1);
                    let r2 = Rational64::new(v2.0, v2.1);
                    let res = r1 - r2;
                    if *res.numer() != 0 {
                        new_dims.push((k1.clone(), (*res.numer(), *res.denom())));
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        while i < self.dimensions.len() {
            new_dims.push(self.dimensions[i].clone());
            i += 1;
        }
        while j < other.dimensions.len() {
            let (k2, v2) = &other.dimensions[j];
            new_dims.push((k2.clone(), (-v2.0, v2.1)));
            j += 1;
        }
        let id = Self::calculate_id(&new_dims);
        RationalUnit { dimensions: new_dims, id }
    }

    pub fn pow(&self, exp_r: Rational64) -> Self {
        let mut new_dims = DimVec::new();
        for (base, (num, den)) in &self.dimensions {
            let base_r = Rational64::new(*num, *den);
            let res = base_r * exp_r;
            if *res.numer() != 0 {
                new_dims.push((base.clone(), (*res.numer(), *res.denom())));
            }
        }
        let id = Self::calculate_id(&new_dims);
        RationalUnit { dimensions: new_dims, id }
    }

    pub fn __repr__(&self) -> String {
        if self.dimensions.is_empty() {
            return "Dimensionless".to_string();
        }
        let mut parts = Vec::new();
        for (base, (num, den)) in &self.dimensions {
            if *num == 1 && *den == 1 {
                parts.push(base.clone());
            } else if *den == 1 {
                parts.push(format!("{}^{}", base, num));
            } else {
                parts.push(format!("{}^{}/{}", base, num, den));
            }
        }
        parts.join(" * ")
    }

    pub fn __eq__(&self, other: &RationalUnit) -> bool {
        self.id == other.id
    }

    pub fn __hash__(&self) -> u64 {
        self.id
    }

    pub fn to_string(&self, _system: Option<()>, _use_alias: bool, _alias_preference: Option<()>) -> String {
        self.__repr__()
    }
}
