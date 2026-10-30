//! Deterministic checks for generated tasks, learner answers and visual parameters. Each check gives narrow, real
//! evidence — recomputed arithmetic, dimensional consistency, a value matched against a frozen key, agreement of two
//! expressions at sampled points, a catalog constraint — and is recorded separately from any model judgement.
//! Sampled agreement is not a proof, and text the parser cannot read is "inconclusive", never "wrong".
use crate::contracts::{AnswerKey, CheckResult};

const MAX_INPUT: usize = 400;
const MAX_NODES: usize = 400;
const MAX_DEPTH: usize = 60;

pub fn result(id: &str, label: &str, outcome: &str, detail: impl Into<String>) -> CheckResult {
    CheckResult {
        id: id.into(),
        label: label.into(),
        outcome: outcome.into(),
        detail: detail.into(),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Units: exponents of (m, kg, s, A, K, mol, cd) plus a scale to SI. Affine temperature scales are flagged.

pub type Dim = [i8; 7];
const NONE: Dim = [0; 7];

#[derive(Clone, Copy, Debug)]
pub struct Unit {
    pub scale: f64,
    pub dim: Dim,
    pub affine: bool,
}

fn dim(m: i8, kg: i8, s: i8, a: i8, k: i8, mol: i8) -> Dim {
    [m, kg, s, a, k, mol, 0]
}

fn base_unit(symbol: &str) -> Option<Unit> {
    let u = |scale: f64, dim: Dim| {
        Some(Unit {
            scale,
            dim,
            affine: false,
        })
    };
    match symbol {
        "m" => u(1.0, dim(1, 0, 0, 0, 0, 0)),
        "g" => u(1e-3, dim(0, 1, 0, 0, 0, 0)),
        "s" | "sec" => u(1.0, dim(0, 0, 1, 0, 0, 0)),
        "A" => u(1.0, dim(0, 0, 0, 1, 0, 0)),
        "K" => u(1.0, dim(0, 0, 0, 0, 1, 0)),
        "mol" => u(1.0, dim(0, 0, 0, 0, 0, 1)),
        "cd" => u(1.0, [0, 0, 0, 0, 0, 0, 1]),
        "N" => u(1.0, dim(1, 1, -2, 0, 0, 0)),
        "J" => u(1.0, dim(2, 1, -2, 0, 0, 0)),
        "W" => u(1.0, dim(2, 1, -3, 0, 0, 0)),
        "Pa" => u(1.0, dim(-1, 1, -2, 0, 0, 0)),
        "C" => u(1.0, dim(0, 0, 1, 1, 0, 0)),
        "V" => u(1.0, dim(2, 1, -3, -1, 0, 0)),
        "Ω" | "ohm" => u(1.0, dim(2, 1, -3, -2, 0, 0)),
        "F" => u(1.0, dim(-2, -1, 4, 2, 0, 0)),
        "T" => u(1.0, dim(0, 1, -2, -1, 0, 0)),
        "Wb" => u(1.0, dim(2, 1, -2, -1, 0, 0)),
        "H" => u(1.0, dim(2, 1, -2, -2, 0, 0)),
        "S" => u(1.0, dim(-2, -1, 3, 2, 0, 0)),
        "Hz" => u(1.0, dim(0, 0, -1, 0, 0, 0)),
        "L" => u(1e-3, dim(3, 0, 0, 0, 0, 0)),
        "M" => u(1e3, dim(-3, 0, 0, 0, 0, 1)),
        "eV" => u(1.602_176_634e-19, dim(2, 1, -2, 0, 0, 0)),
        "cal" => u(4.184, dim(2, 1, -2, 0, 0, 0)),
        "atm" => u(101_325.0, dim(-1, 1, -2, 0, 0, 0)),
        "bar" => u(1e5, dim(-1, 1, -2, 0, 0, 0)),
        "Torr" | "torr" => u(101_325.0 / 760.0, dim(-1, 1, -2, 0, 0, 0)),
        "mmHg" => u(133.322_387_415, dim(-1, 1, -2, 0, 0, 0)),
        "min" => u(60.0, dim(0, 0, 1, 0, 0, 0)),
        "h" | "hr" => u(3600.0, dim(0, 0, 1, 0, 0, 0)),
        "day" => u(86_400.0, dim(0, 0, 1, 0, 0, 0)),
        "Da" | "amu" => u(1.660_539_066_6e-27, dim(0, 1, 0, 0, 0, 0)),
        "ft" => u(0.3048, dim(1, 0, 0, 0, 0, 0)),
        "mi" => u(1609.344, dim(1, 0, 0, 0, 0, 0)),
        "lb" => u(0.453_592_37, dim(0, 1, 0, 0, 0, 0)),
        "rad" | "sr" => u(1.0, NONE),
        "deg" | "°" => u(std::f64::consts::PI / 180.0, NONE),
        "%" => u(0.01, NONE),
        // Inside a compound unit (J/(g·°C)) a degree is a temperature difference; see `parse_unit` for a lone scale.
        "°C" | "degC" => u(1.0, dim(0, 0, 0, 0, 1, 0)),
        "°F" | "degF" => u(5.0 / 9.0, dim(0, 0, 0, 0, 1, 0)),
        _ => None,
    }
}

fn prefix(p: &str) -> Option<f64> {
    Some(match p {
        "Y" => 1e24,
        "Z" => 1e21,
        "E" => 1e18,
        "P" => 1e15,
        "T" => 1e12,
        "G" => 1e9,
        "M" => 1e6,
        "k" => 1e3,
        "h" => 1e2,
        "da" => 1e1,
        "d" => 1e-1,
        "c" => 1e-2,
        "m" => 1e-3,
        "µ" | "μ" | "u" => 1e-6,
        "n" => 1e-9,
        "p" => 1e-12,
        "f" => 1e-15,
        "a" => 1e-18,
        _ => return None,
    })
}

fn symbol_unit(symbol: &str) -> Option<Unit> {
    if let Some(u) = base_unit(symbol) {
        return Some(u);
    }
    // A prefix applies only to an ordinary (non-affine) unit symbol, never to a whole word such as "min".
    for split in [2, 1] {
        if symbol.chars().count() > split {
            let p: String = symbol.chars().take(split).collect();
            let rest: String = symbol.chars().skip(split).collect();
            if let (Some(f), Some(u)) = (prefix(&p), base_unit(&rest))
                && !u.affine
                && ![
                    "%", "°", "deg", "rad", "min", "day", "h", "hr", "atm", "mi", "ft", "lb",
                ]
                .contains(&rest.as_str())
            {
                return Some(Unit {
                    scale: f * u.scale,
                    ..u
                });
            }
        }
    }
    None
}

fn superscript(c: char) -> Option<char> {
    Some(match c {
        '⁰' => '0',
        '¹' => '1',
        '²' => '2',
        '³' => '3',
        '⁴' => '4',
        '⁵' => '5',
        '⁶' => '6',
        '⁷' => '7',
        '⁸' => '8',
        '⁹' => '9',
        '⁻' => '-',
        _ => return None,
    })
}

/// Parses a unit such as `m/s^2`, `kg·m²/s²`, `J/(mol·K)`, `mol L⁻¹`, `km/h` or `°C`. After a `/`, the following
/// multiplied terms are in the denominator (so `J/mol·K` reads as J/(mol·K)). Empty or `1` is dimensionless.
pub fn parse_unit(text: &str) -> Option<Unit> {
    let t = text.trim();
    if t.is_empty() || ["1", "-", "dimensionless", "unitless", "none", "no unit"].contains(&t) {
        return Some(Unit {
            scale: 1.0,
            dim: NONE,
            affine: false,
        });
    }
    if t.chars().count() > 60 {
        return None;
    }
    // A lone Celsius or Fahrenheit reading is an affine scale: values in it are not converted automatically.
    if ["°C", "degC", "°F", "degF"].contains(&t) {
        return base_unit(t).map(|u| Unit { affine: true, ..u });
    }
    if let Some(rest) = t.strip_prefix("1/").or_else(|| t.strip_prefix("1 /")) {
        let u = parse_unit(rest)?;
        let mut d = u.dim;
        for e in d.iter_mut() {
            *e = e.checked_neg()?;
        }
        return Some(Unit {
            scale: 1.0 / u.scale,
            dim: d,
            affine: false,
        });
    }
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0;
    let unit = unit_group(&chars, &mut i, 0)?;
    (i == chars.len()).then_some(unit)
}

fn combine(a: Unit, b: Unit, sign: i8) -> Option<Unit> {
    let mut d = a.dim;
    for (k, dimension) in d.iter_mut().enumerate() {
        *dimension = dimension.checked_add(b.dim[k].checked_mul(sign)?)?;
    }
    Some(Unit {
        scale: if sign > 0 {
            a.scale * b.scale
        } else {
            a.scale / b.scale
        },
        dim: d,
        affine: false,
    })
}

fn unit_group(c: &[char], i: &mut usize, depth: usize) -> Option<Unit> {
    if depth > 4 {
        return None;
    }
    let mut acc = Unit {
        scale: 1.0,
        dim: NONE,
        affine: false,
    };
    let mut denominator = false;
    let mut first = true;
    loop {
        while *i < c.len() && c[*i] == ' ' {
            *i += 1;
        }
        if *i >= c.len() || c[*i] == ')' {
            break;
        }
        if !first {
            match c[*i] {
                '/' => {
                    denominator = true;
                    *i += 1;
                }
                '*' | '·' | '⋅' | '.' | '×' => *i += 1,
                _ => {}
            }
            while *i < c.len() && c[*i] == ' ' {
                *i += 1;
            }
        }
        let term = if *i < c.len() && c[*i] == '(' {
            *i += 1;
            let g = unit_group(c, i, depth + 1)?;
            if *i >= c.len() || c[*i] != ')' {
                return None;
            }
            *i += 1;
            g
        } else {
            let start = *i;
            while *i < c.len()
                && (c[*i].is_alphabetic() || ['°', 'Ω', 'µ', 'μ', '%'].contains(&c[*i]))
            {
                *i += 1;
            }
            if start == *i {
                return None;
            }
            let symbol: String = c[start..*i].iter().collect();
            symbol_unit(&symbol)?
        };
        // Exponent: ^n, ^-n, or superscript digits.
        let mut exponent: i8 = 1;
        if *i < c.len() && c[*i] == '^' {
            *i += 1;
            let start = *i;
            if *i < c.len() && (c[*i] == '-' || c[*i] == '+') {
                *i += 1;
            }
            while *i < c.len() && c[*i].is_ascii_digit() {
                *i += 1;
            }
            exponent = c[start..*i].iter().collect::<String>().parse().ok()?;
        } else if *i < c.len() && superscript(c[*i]).is_some() {
            let mut s = String::new();
            while *i < c.len()
                && let Some(d) = superscript(c[*i])
            {
                s.push(d);
                *i += 1;
            }
            exponent = s.parse().ok()?;
        }
        let mut powered = Unit {
            scale: term.scale.powi(exponent as i32),
            dim: term.dim,
            affine: term.affine,
        };
        for k in 0..7 {
            powered.dim[k] = term.dim[k].checked_mul(exponent)?;
        }
        acc = combine(acc, powered, if denominator { -1 } else { 1 })?;
        first = false;
    }
    Some(acc)
}

pub fn dim_text(d: &Dim) -> String {
    let names = ["m", "kg", "s", "A", "K", "mol", "cd"];
    let parts: Vec<String> = d
        .iter()
        .zip(names)
        .filter(|(e, _)| **e != 0)
        .map(|(e, n)| {
            if *e == 1 {
                n.to_string()
            } else {
                format!("{n}^{e}")
            }
        })
        .collect();
    if parts.is_empty() {
        "dimensionless".into()
    } else {
        parts.join("·")
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Numbers in prose: 12, -3.5, 1.2e-3, 6.022×10²³, 6.022 x 10^23, 1/2 is two numbers.

pub fn numbers(text: &str) -> Vec<f64> {
    let t: String = text
        .chars()
        .map(|c| match c {
            '−' | '–' => '-',
            '\u{2009}' | '\u{202f}' | '\u{00a0}' => ' ',
            c => c,
        })
        .collect();
    let c: Vec<char> = t.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < c.len() && out.len() < 200 {
        let starts = c[i].is_ascii_digit()
            || (c[i] == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit()));
        if !starts {
            i += 1;
            continue;
        }
        // A sign belongs to the number when it is not a binary minus between two operands.
        let negative =
            i > 0 && c[i - 1] == '-' && (i < 2 || !(c[i - 2].is_alphanumeric() || c[i - 2] == ')'));
        let start = i;
        while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.' || c[i] == ',') {
            // A comma is a thousands separator only when exactly three digits follow it.
            if c[i] == ',' {
                let digits = c[i + 1..].iter().take_while(|d| d.is_ascii_digit()).count();
                if digits != 3 {
                    break;
                }
            }
            i += 1;
        }
        let mut mantissa: String = c[start..i].iter().filter(|ch| **ch != ',').collect();
        if mantissa.ends_with('.') {
            mantissa.pop();
        }
        let Ok(mut value) = mantissa.parse::<f64>() else {
            continue;
        };
        // e-notation directly after the mantissa.
        if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
            let mut j = i + 1;
            if j < c.len() && (c[j] == '-' || c[j] == '+') {
                j += 1;
            }
            let digits = j;
            while j < c.len() && c[j].is_ascii_digit() {
                j += 1;
            }
            if j > digits && !c.get(j).is_some_and(|ch| ch.is_alphabetic()) {
                let e: String = c[i + 1..j].iter().collect();
                if let Ok(e) = e.parse::<i32>() {
                    value *= 10f64.powi(e);
                    i = j;
                }
            }
        }
        // "× 10^n", "x10^-3", "·10⁻³"
        let mut j = i;
        while j < c.len() && c[j] == ' ' {
            j += 1;
        }
        if j < c.len() && ['×', 'x', '*', '·'].contains(&c[j]) {
            let mut k = j + 1;
            while k < c.len() && c[k] == ' ' {
                k += 1;
            }
            if c.get(k) == Some(&'1') && c.get(k + 1) == Some(&'0') {
                let mut m = k + 2;
                let mut exp = String::new();
                if c.get(m) == Some(&'^') {
                    m += 1;
                    if c.get(m) == Some(&'(') {
                        m += 1;
                    }
                    while m < c.len()
                        && (c[m].is_ascii_digit()
                            || (exp.is_empty() && (c[m] == '-' || c[m] == '+')))
                    {
                        exp.push(c[m]);
                        m += 1;
                    }
                    if c.get(m) == Some(&')') {
                        m += 1;
                    }
                } else {
                    while m < c.len()
                        && let Some(d) = superscript(c[m])
                    {
                        exp.push(d);
                        m += 1;
                    }
                }
                if let Ok(e) = exp.parse::<i32>() {
                    value *= 10f64.powi(e);
                    i = m;
                }
            }
        }
        out.push(if negative { -value } else { value });
    }
    out
}

pub fn close(a: f64, b: f64, relative: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let scale = a.abs().max(b.abs());
    (a - b).abs() <= relative * scale || (a - b).abs() <= 1e-12
}

// ---------------------------------------------------------------------------------------------------------------
// Expressions: a bounded parser for the reference formulas and for learners' own notation.

#[derive(Clone, Debug)]
enum Node {
    Num(f64),
    Var(usize),
    Neg(Box<Node>),
    Bin(char, Box<Node>, Box<Node>),
    Fun(&'static str, Box<Node>),
}

const FUNCTIONS: &[&str] = &[
    "sqrt", "cbrt", "abs", "exp", "ln", "log10", "log2", "log", "sin", "cos", "tan", "sec", "csc",
    "cot", "asin", "acos", "atan", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
];

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Op(char),
}

pub struct Expr {
    root: Node,
    /// The text used `log` without a base, which this checker will not guess.
    pub ambiguous_log: bool,
}

/// Normalizes common learner notation: Unicode operators and minus signs, superscript powers, √, π and `**`.
fn normalize(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '−' | '–' => out.push('-'),
            '·' | '×' | '⋅' | '∗' => out.push('*'),
            '÷' => out.push('/'),
            'π' => out.push_str(" pi "),
            '√' => out.push_str(" sqrt "),
            '\u{2009}' | '\u{202f}' | '\u{00a0}' => out.push(' '),
            c if superscript(c).is_some() => {
                let mut s = String::from(superscript(c).unwrap());
                while let Some(&n) = chars.peek() {
                    if let Some(d) = superscript(n) {
                        s.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push_str(&format!("^({s})"));
            }
            _ => out.push(c),
        }
    }
    out.replace("**", "^")
}

fn tokens(text: &str) -> Option<Vec<Tok>> {
    let c: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit()
            || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                let mut j = i + 1;
                if j < c.len() && (c[j] == '-' || c[j] == '+') {
                    j += 1;
                }
                let digits = j;
                while j < c.len() && c[j].is_ascii_digit() {
                    j += 1;
                }
                if j > digits && !c.get(j).is_some_and(|x| x.is_alphabetic()) {
                    i = j;
                }
            }
            out.push(Tok::Num(
                c[start..i].iter().collect::<String>().parse().ok()?,
            ));
        } else if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Id(c[start..i].iter().collect()));
        } else if "+-*/^()".contains(ch) {
            out.push(Tok::Op(ch));
            i += 1;
        } else if ch == '[' || ch == '{' {
            out.push(Tok::Op('('));
            i += 1;
        } else if ch == ']' || ch == '}' {
            out.push(Tok::Op(')'));
            i += 1;
        } else {
            return None;
        }
        if out.len() > MAX_NODES {
            return None;
        }
    }
    Some(out)
}

struct Parser<'a> {
    t: Vec<Tok>,
    i: usize,
    vars: &'a [String],
    nodes: usize,
    ambiguous_log: bool,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn count(&mut self) -> Option<()> {
        self.nodes += 1;
        (self.nodes <= MAX_NODES).then_some(())
    }
    fn sum(&mut self, depth: usize) -> Option<Node> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut left = self.product(depth + 1)?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.i += 1;
            let right = self.product(depth + 1)?;
            self.count()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
        Some(left)
    }
    fn starts_atom(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Num(_)) | Some(Tok::Id(_)) | Some(Tok::Op('('))
        )
    }
    fn product(&mut self, depth: usize) -> Option<Node> {
        let mut left = self.unary(depth + 1)?;
        loop {
            match self.peek().cloned() {
                Some(Tok::Op(op @ ('*' | '/'))) => {
                    self.i += 1;
                    let right = self.unary(depth + 1)?;
                    self.count()?;
                    left = Node::Bin(op, Box::new(left), Box::new(right));
                }
                // Implicit multiplication: 2x, 3(x+1), (x+1)(x-1), x y.
                _ if self.starts_atom() => {
                    let right = self.power(depth + 1)?;
                    self.count()?;
                    left = Node::Bin('*', Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Some(left)
    }
    fn unary(&mut self, depth: usize) -> Option<Node> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.i += 1;
                self.count()?;
                Some(Node::Neg(Box::new(self.unary(depth + 1)?)))
            }
            Some(Tok::Op('+')) => {
                self.i += 1;
                self.unary(depth + 1)
            }
            _ => self.power(depth + 1),
        }
    }
    fn power(&mut self, depth: usize) -> Option<Node> {
        if depth > MAX_DEPTH {
            return None;
        }
        let base = self.atom(depth + 1)?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.i += 1;
            // Right associative; the exponent may carry its own sign.
            let exponent = self.unary(depth + 1)?;
            self.count()?;
            return Some(Node::Bin('^', Box::new(base), Box::new(exponent)));
        }
        Some(base)
    }
    fn atom(&mut self, depth: usize) -> Option<Node> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.count()?;
        match self.peek().cloned()? {
            Tok::Num(v) => {
                self.i += 1;
                Some(Node::Num(v))
            }
            Tok::Op('(') => {
                self.i += 1;
                let inner = self.sum(depth + 1)?;
                if self.peek() != Some(&Tok::Op(')')) {
                    return None;
                }
                self.i += 1;
                Some(inner)
            }
            Tok::Id(name) => {
                self.i += 1;
                self.identifier(&name, depth)
            }
            Tok::Op(_) => None,
        }
    }
    fn identifier(&mut self, name: &str, depth: usize) -> Option<Node> {
        if let Some(k) = self.vars.iter().position(|v| v == name) {
            return Some(Node::Var(k));
        }
        if let Some(f) = FUNCTIONS.iter().find(|f| **f == name) {
            if *f == "log" {
                self.ambiguous_log = true;
            }
            // sin x², sin(x): a function applies to the following power expression.
            let argument = self.power(depth + 1)?;
            return Some(Node::Fun(f, Box::new(argument)));
        }
        if name == "pi" {
            return Some(Node::Num(std::f64::consts::PI));
        }
        if name == "e" {
            return Some(Node::Num(std::f64::consts::E));
        }
        // "xy" with declared variables x and y is a product of single-letter variables.
        let letters: Vec<String> = name.chars().map(|c| c.to_string()).collect();
        if letters.len() > 1 && letters.len() <= 6 && letters.iter().all(|l| self.vars.contains(l))
        {
            let mut node = Node::Var(self.vars.iter().position(|v| *v == letters[0])?);
            for l in &letters[1..] {
                self.count()?;
                node = Node::Bin(
                    '*',
                    Box::new(node),
                    Box::new(Node::Var(self.vars.iter().position(|v| v == l)?)),
                );
            }
            return Some(node);
        }
        None
    }
}

/// Parses an expression in the given variables. An equation keeps only its right-hand side (`f'(x) = 2x` → `2x`).
pub fn parse_expr(text: &str, vars: &[String]) -> Option<Expr> {
    if text.chars().count() > MAX_INPUT {
        return None;
    }
    let rhs = text
        .rsplit('=')
        .next()
        .unwrap_or(text)
        .trim()
        .trim_end_matches(['.', ',', ';']);
    let tok = tokens(&normalize(rhs))?;
    if tok.is_empty() {
        return None;
    }
    let mut p = Parser {
        t: tok,
        i: 0,
        vars,
        nodes: 0,
        ambiguous_log: false,
    };
    let root = p.sum(0)?;
    (p.i == p.t.len()).then_some(Expr {
        root,
        ambiguous_log: p.ambiguous_log,
    })
}

fn apply(f: &str, x: f64) -> f64 {
    match f {
        "sqrt" => x.sqrt(),
        "cbrt" => x.cbrt(),
        "abs" => x.abs(),
        "exp" => x.exp(),
        "ln" | "log" => x.ln(),
        "log10" => x.log10(),
        "log2" => x.log2(),
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "sec" => 1.0 / x.cos(),
        "csc" => 1.0 / x.sin(),
        "cot" => 1.0 / x.tan(),
        "asin" | "arcsin" => x.asin(),
        "acos" | "arccos" => x.acos(),
        "atan" | "arctan" => x.atan(),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        _ => f64::NAN,
    }
}

impl Expr {
    pub fn eval(&self, values: &[f64]) -> f64 {
        fn go(n: &Node, v: &[f64]) -> f64 {
            match n {
                Node::Num(x) => *x,
                Node::Var(k) => v.get(*k).copied().unwrap_or(f64::NAN),
                Node::Neg(a) => -go(a, v),
                Node::Fun(f, a) => apply(f, go(a, v)),
                Node::Bin(op, a, b) => {
                    let (x, y) = (go(a, v), go(b, v));
                    match op {
                        '+' => x + y,
                        '-' => x - y,
                        '*' => x * y,
                        '/' => x / y,
                        '^' => x.powf(y),
                        _ => f64::NAN,
                    }
                }
            }
        }
        go(&self.root, values)
    }
    /// Value and SI dimension, given each variable's SI value and dimension. Adding unlike dimensions, a function of
    /// a dimensional quantity, or a fractional power of a dimension is an error with a plain reason.
    pub fn eval_dim(&self, values: &[(f64, Dim)]) -> Result<(f64, Dim), String> {
        fn go(n: &Node, v: &[(f64, Dim)]) -> Result<(f64, Dim), String> {
            Ok(match n {
                Node::Num(x) => (*x, NONE),
                Node::Var(k) => *v.get(*k).ok_or("unknown variable")?,
                Node::Neg(a) => {
                    let (x, d) = go(a, v)?;
                    (-x, d)
                }
                Node::Fun(f, a) => {
                    let (x, d) = go(a, v)?;
                    match *f {
                        "abs" => (x.abs(), d),
                        "sqrt" | "cbrt" => {
                            let root = if *f == "sqrt" { 2 } else { 3 };
                            if d.iter().any(|e| e % root != 0) {
                                return Err(format!(
                                    "{f} of a quantity in {} has no whole-unit result",
                                    dim_text(&d)
                                ));
                            }
                            let mut r = d;
                            for e in r.iter_mut() {
                                *e /= root;
                            }
                            (apply(f, x), r)
                        }
                        _ if d != NONE => {
                            return Err(format!(
                                "{f} needs a dimensionless argument, not {}",
                                dim_text(&d)
                            ));
                        }
                        _ => (apply(f, x), NONE),
                    }
                }
                Node::Bin(op, a, b) => {
                    let (x, dx) = go(a, v)?;
                    let (y, dy) = go(b, v)?;
                    match op {
                        '+' | '-' => {
                            if dx != dy {
                                return Err(format!("adds {} to {}", dim_text(&dx), dim_text(&dy)));
                            }
                            (if *op == '+' { x + y } else { x - y }, dx)
                        }
                        '*' | '/' => {
                            let sign = if *op == '*' { 1 } else { -1 };
                            let mut d = dx;
                            for k in 0..7 {
                                d[k] = dx[k]
                                    .checked_add(dy[k] * sign)
                                    .ok_or("dimension overflow")?;
                            }
                            (if *op == '*' { x * y } else { x / y }, d)
                        }
                        _ => {
                            if dy != NONE {
                                return Err("an exponent must be dimensionless".into());
                            }
                            if dx == NONE {
                                (x.powf(y), NONE)
                            } else {
                                let half = (y * 2.0).round();
                                if (y * 2.0 - half).abs() > 1e-9 {
                                    return Err(
                                        "a dimensional base needs a whole or half exponent".into(),
                                    );
                                }
                                let mut d = dx;
                                for k in 0..7 {
                                    let scaled = dx[k] as f64 * y;
                                    if (scaled - scaled.round()).abs() > 1e-9 {
                                        return Err("the power leaves a fractional unit".into());
                                    }
                                    d[k] = scaled.round() as i8;
                                }
                                (x.powf(y), d)
                            }
                        }
                    }
                }
            })
        }
        go(&self.root, values)
    }
}

/// Deterministic pseudo-random sample points (no system randomness, so a check can be reproduced).
fn samples(variables: &[(f64, f64)], count: usize) -> Vec<Vec<f64>> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    (0..count)
        .map(|_| {
            variables
                .iter()
                .map(|(lo, hi)| lo + (hi - lo) * next())
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Checks on a generated task before it is shown.

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic())
        && name.chars().count() <= 24
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !FUNCTIONS.contains(&name)
        && !["pi", "e"].contains(&name)
}

/// Checks an answer key against the task text. A failed check means the task must not be shown.
pub fn check_key(key: &AnswerKey, task_text: &str) -> Vec<CheckResult> {
    let mut out = vec![];
    match key.kind.as_str() {
        "none" => out.push(result(
            "answer-key",
            "Answer key",
            "not_applicable",
            "Judged work (explanation, proof, derivation or code reasoning): graded against the frozen rubric, with no automated answer check.",
        )),
        "numeric" => {
            let names: Vec<String> = key.givens.iter().map(|g| g.name.clone()).collect();
            if key.givens.len() > 12
                || names.iter().any(|n| !valid_name(n))
                || names.iter().enumerate().any(|(i, n)| names[..i].contains(n))
            {
                out.push(result("givens", "Given quantities", "failed", "The givens need distinct plain names (letters, digits, underscores)."));
                return out;
            }
            // Every given must be stated in the task or its assumptions, or the task is not solvable as written.
            let stated = numbers(task_text);
            let missing: Vec<String> = key
                .givens
                .iter()
                .filter(|g| !stated.iter().any(|s| close(*s, g.value, 1e-9)))
                .map(|g| format!("{} = {}", g.name, g.value))
                .collect();
            out.push(if missing.is_empty() {
                result("givens-stated", "Every given value appears in the task", "passed", format!("{} given value(s) found in the statement or assumptions.", key.givens.len()))
            } else {
                result("givens-stated", "Every given value appears in the task", "failed", format!("Not stated in the task: {}.", missing.join(", ")))
            });
            let Some(expr) = parse_expr(&key.expression, &names).filter(|e| !e.ambiguous_log) else {
                out.push(result("formula", "Reference formula", "failed", "The reference formula could not be read (use ln or log10, never a bare log)."));
                return out;
            };
            let Some(value) = key.value.filter(|v| v.is_finite()) else {
                out.push(result("formula", "Reference formula", "failed", "The answer key has no finite value."));
                return out;
            };
            let mut si = vec![];
            for g in &key.givens {
                let Some(u) = parse_unit(&g.unit) else {
                    out.push(result("units", "Units are consistent", "inconclusive", format!("The unit “{}” of {} is not recognised, so units were not checked.", g.unit, g.name)));
                    si.clear();
                    break;
                };
                if u.affine {
                    out.push(result("units", "Units are consistent", "inconclusive", "Temperature scales with an offset (°C, °F) are not converted; units were not checked."));
                    si.clear();
                    break;
                }
                si.push((g.value * u.scale, u.dim));
            }
            let answer_unit = parse_unit(&key.unit);
            if si.len() == key.givens.len() {
                match (expr.eval_dim(&si), answer_unit) {
                    (Ok((computed, d)), Some(u)) if !u.affine => {
                        // Details are shown with the task before any answer, so they never state the answer itself.
                        out.push(if d == u.dim {
                            result("units", "Units are consistent", "passed", format!("The reference formula's units ({}) match the unit the task asks for{}.", dim_text(&d), if key.unit.is_empty() { String::new() } else { format!(" ({})", key.unit) }))
                        } else {
                            result("units", "Units are consistent", "failed", format!("The reference formula gives {} but the answer is stated in {} ({}).", dim_text(&d), key.unit, dim_text(&u.dim)))
                        });
                        let stated = value * u.scale;
                        out.push(if close(computed, stated, key.tolerance.clamp(0.006, 0.05)) {
                            result("arithmetic", "Reference answer recomputed", "passed", "Recomputing the reference formula from the stated values reproduces the reference answer.")
                        } else {
                            result("arithmetic", "Reference answer recomputed", "failed", "Recomputing the reference formula from the stated values does not reproduce the reference answer.")
                        });
                    }
                    (Err(reason), _) => out.push(result("units", "Units are consistent", "failed", format!("The reference formula {reason}."))),
                    _ => out.push(result("units", "Units are consistent", "inconclusive", format!("The answer unit “{}” is not recognised, so units were not checked.", key.unit))),
                }
            } else {
                // Without units the arithmetic can still be recomputed in the stated values.
                let computed = expr.eval(&key.givens.iter().map(|g| g.value).collect::<Vec<_>>());
                out.push(if close(computed, value, key.tolerance.clamp(0.006, 0.05)) {
                    result("arithmetic", "Reference answer recomputed", "passed", "Recomputing the reference formula from the stated values reproduces the reference answer.")
                } else {
                    result("arithmetic", "Reference answer recomputed", "failed", "Recomputing the reference formula from the stated values does not reproduce the reference answer.")
                });
            }
        }
        "expression" => {
            let names: Vec<String> = key.variables.iter().map(|v| v.name.clone()).collect();
            if key.variables.is_empty()
                || key.variables.len() > 4
                || names.iter().any(|n| !valid_name(n))
                || key.variables.iter().any(|v| !(v.min.is_finite() && v.max.is_finite() && v.min < v.max))
            {
                out.push(result("domain", "Stated domain", "failed", "The expression key needs one to four named variables with a finite range each."));
                return out;
            }
            let Some(expr) = parse_expr(&key.expression, &names).filter(|e| !e.ambiguous_log) else {
                out.push(result("formula", "Reference expression", "failed", "The reference expression could not be read (use ln or log10, never a bare log)."));
                return out;
            };
            let points = samples(&key.variables.iter().map(|v| (v.min, v.max)).collect::<Vec<_>>(), 16);
            let finite = points.iter().filter(|p| expr.eval(p).is_finite()).count();
            out.push(if finite >= 8 {
                result("domain", "Reference expression is defined on its domain", "passed", format!("Finite at {finite} of 16 sampled points in the stated domain."))
            } else {
                result("domain", "Reference expression is defined on its domain", "failed", format!("Finite at only {finite} of 16 sampled points; the domain or expression is wrong."))
            });
        }
        _ => out.push(result("answer-key", "Answer key", "failed", "Unknown answer key kind.")),
    }
    out
}

pub fn short(x: f64) -> String {
    if x == 0.0 || !x.is_finite() {
        return format!("{x}");
    }
    let magnitude = x.abs().log10().floor() as i32;
    if !(-4..6).contains(&magnitude) {
        return format!("{x:.4e}");
    }
    let decimals = (4 - magnitude).clamp(0, 8) as usize;
    let s = format!("{x:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    };
    if s == "-0" { "0".into() } else { s }
}

/// Hints must not hand over the final value; such a hint is removed before the task is frozen.
pub fn hint_leaks(key: &AnswerKey, hint: &str) -> bool {
    key.kind == "numeric"
        && key
            .value
            .is_some_and(|v| v != 0.0 && numbers(hint).iter().any(|n| close(*n, v, 1e-6)))
}

// ---------------------------------------------------------------------------------------------------------------
// Checks on a learner's answer after the tutor quotes it. The tutor proposes which text is the final answer; the
// core recomputes everything else from that exact text.

/// The quoted unit and value must come from the learner's own excerpt; otherwise the proposal is fabricated.
pub fn quoted_value_present(excerpt: &str, value: f64) -> bool {
    numbers(excerpt).iter().any(|n| close(*n, value, 1e-9))
}

pub fn check_answer(
    key: &AnswerKey,
    value: Option<f64>,
    unit: &str,
    expression: &str,
) -> CheckResult {
    let label = "Final answer checked against the frozen key";
    match key.kind.as_str() {
        "numeric" => {
            let (Some(expected), Some(given)) = (key.value, value) else {
                return result(
                    "final-answer",
                    label,
                    "inconclusive",
                    "No final numerical value was identified in the answer.",
                );
            };
            let Some(key_unit) = parse_unit(&key.unit) else {
                return result(
                    "final-answer",
                    label,
                    "inconclusive",
                    "The key's unit is not recognised.",
                );
            };
            let tolerance = if key.tolerance > 0.0 {
                key.tolerance.min(0.1)
            } else {
                0.01
            };
            let dimensional = key_unit.dim != NONE;
            if unit.trim().is_empty() && dimensional {
                return if close(given, expected, tolerance) {
                    result(
                        "final-answer",
                        label,
                        "inconclusive",
                        format!(
                            "The value {} matches the key but has no unit (the task asks for {}).",
                            short(given),
                            key.unit
                        ),
                    )
                } else {
                    result(
                        "final-answer",
                        label,
                        "inconclusive",
                        format!(
                            "The value {} has no unit, so it was not compared with the key (the task asks for {}).",
                            short(given),
                            key.unit
                        ),
                    )
                };
            }
            let Some(learner_unit) = parse_unit(unit) else {
                return result(
                    "final-answer",
                    label,
                    "inconclusive",
                    format!("The unit “{unit}” is not recognised; compare it by hand."),
                );
            };
            if learner_unit.dim != key_unit.dim {
                return result(
                    "final-answer",
                    label,
                    "failed",
                    format!(
                        "The answer is in {} ({}), but the task asks for {} ({}).",
                        unit,
                        dim_text(&learner_unit.dim),
                        key.unit,
                        dim_text(&key_unit.dim)
                    ),
                );
            }
            if (learner_unit.affine || key_unit.affine) && unit.trim() != key.unit.trim() {
                return result(
                    "final-answer",
                    label,
                    "inconclusive",
                    "Temperature scales with an offset are not converted; compare by hand.",
                );
            }
            let a = given * learner_unit.scale;
            let b = expected * key_unit.scale;
            if close(a, b, tolerance) {
                result(
                    "final-answer",
                    label,
                    "passed",
                    format!(
                        "{} {} matches the frozen answer {} {} within {}%.",
                        short(given),
                        unit.trim(),
                        short(expected),
                        key.unit,
                        short(tolerance * 100.0)
                    ),
                )
            } else {
                result(
                    "final-answer",
                    label,
                    "failed",
                    format!(
                        "{} {} does not match the frozen answer key (tolerance {}%).",
                        short(given),
                        unit.trim(),
                        short(tolerance * 100.0)
                    ),
                )
            }
        }
        "expression" => {
            let names: Vec<String> = key.variables.iter().map(|v| v.name.clone()).collect();
            if expression.trim().is_empty() {
                return result(
                    "final-answer",
                    label,
                    "inconclusive",
                    "No final expression was identified in the answer.",
                );
            }
            let (Some(reference), Some(learner)) = (
                parse_expr(&key.expression, &names),
                parse_expr(expression, &names),
            ) else {
                return result(
                    "final-answer",
                    label,
                    "inconclusive",
                    "The expression could not be read automatically; the rubric judgement stands alone.",
                );
            };
            if learner.ambiguous_log {
                return result(
                    "final-answer",
                    label,
                    "inconclusive",
                    "“log” without a base is ambiguous here, so the expression was not compared automatically.",
                );
            }
            let points = samples(
                &key.variables
                    .iter()
                    .map(|v| (v.min, v.max))
                    .collect::<Vec<_>>(),
                24,
            );
            let mut compared = 0;
            for p in &points {
                let (x, y) = (reference.eval(p), learner.eval(p));
                if !x.is_finite() || !y.is_finite() {
                    continue;
                }
                compared += 1;
                if !close(x, y, 1e-6) && (x - y).abs() > 1e-9 {
                    let at = names
                        .iter()
                        .zip(p)
                        .map(|(n, v)| format!("{n} = {}", short(*v)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return result(
                        "final-answer",
                        label,
                        "failed",
                        format!(
                            "At {at} your expression gives {}, which differs from the reference.",
                            short(y)
                        ),
                    );
                }
            }
            if compared >= 8 {
                result(
                    "final-answer",
                    label,
                    "passed",
                    format!(
                        "The answer agrees with the reference at {compared} sampled points in the stated domain. Sampled agreement is strong evidence, not a proof."
                    ),
                )
            } else {
                result(
                    "final-answer",
                    label,
                    "inconclusive",
                    "Too few points in the domain gave finite values to compare.",
                )
            }
        }
        _ => result(
            "final-answer",
            label,
            "not_applicable",
            "This task is judged against its rubric; no automated answer check applies.",
        ),
    }
}

/// Evaluates a catalog constraint such as `a*c - b*b` against proposed visual parameters.
pub fn constraint_value(expression: &str, parameters: &[(String, f64)]) -> Option<f64> {
    let names: Vec<String> = parameters.iter().map(|(n, _)| n.clone()).collect();
    let e = parse_expr(expression, &names)?;
    let v = e.eval(&parameters.iter().map(|(_, v)| *v).collect::<Vec<_>>());
    v.is_finite().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{DomainVariable, Given};
    fn key(givens: &[(&str, f64, &str)], expression: &str, value: f64, unit: &str) -> AnswerKey {
        AnswerKey {
            kind: "numeric".into(),
            givens: givens
                .iter()
                .map(|(n, v, u)| Given {
                    name: (*n).into(),
                    value: *v,
                    unit: (*u).into(),
                })
                .collect(),
            expression: expression.into(),
            value: Some(value),
            unit: unit.into(),
            tolerance: 0.01,
            variables: vec![],
        }
    }
    #[test]
    fn units_parse_and_convert() {
        let n = parse_unit("kg·m/s²").unwrap();
        assert_eq!(n.dim, parse_unit("N").unwrap().dim);
        assert!((parse_unit("km/h").unwrap().scale - 1000.0 / 3600.0).abs() < 1e-12);
        assert_eq!(
            parse_unit("J/(mol·K)").unwrap().dim,
            parse_unit("J/mol·K").unwrap().dim
        );
        assert_eq!(
            parse_unit("mol L⁻¹").unwrap().dim,
            parse_unit("M").unwrap().dim
        );
        assert!((parse_unit("mM").unwrap().scale - 1.0).abs() < 1e-12);
        assert!((parse_unit("kPa").unwrap().scale - 1000.0).abs() < 1e-9);
        assert_eq!(parse_unit("min").unwrap().scale, 60.0);
        assert!(parse_unit("°C").unwrap().affine);
        assert!(!parse_unit("J/(g·°C)").unwrap().affine);
        assert_eq!(
            parse_unit("1/s").unwrap().dim,
            parse_unit("Hz").unwrap().dim
        );
        assert!(parse_unit("furlongs").is_none());
    }
    #[test]
    fn numbers_in_prose() {
        assert_eq!(numbers("v = 14.0 m/s after 3 s"), vec![14.0, 3.0]);
        let n = numbers("N_A = 6.022×10²³ mol⁻¹ and 1.2e-3 M");
        assert!(close(n[0], 6.022e23, 1e-12) && close(n[1], 1.2e-3, 1e-12));
        assert_eq!(numbers("x = -2 and 5-3"), vec![-2.0, 5.0, 3.0]);
        assert_eq!(numbers("1,000 kg"), vec![1000.0]);
    }
    #[test]
    fn task_key_checks_arithmetic_units_and_statement() {
        let k = key(&[("d", 12.0, "km"), ("t", 0.5, "h")], "d/t", 24.0, "km/h");
        let checks = check_key(
            &k,
            "A cyclist rides 12 km in 0.5 h. Find the average speed.",
        );
        assert!(checks.iter().all(|c| c.outcome == "passed"), "{checks:?}");
        let wrong_unit = key(&[("d", 12.0, "km"), ("t", 0.5, "h")], "d/t", 24.0, "km");
        assert!(
            check_key(&wrong_unit, "12 km in 0.5 h")
                .iter()
                .any(|c| c.id == "units" && c.outcome == "failed")
        );
        let wrong_value = key(&[("d", 12.0, "km"), ("t", 0.5, "h")], "d/t", 20.0, "km/h");
        assert!(
            check_key(&wrong_value, "12 km in 0.5 h")
                .iter()
                .any(|c| c.id == "arithmetic" && c.outcome == "failed")
        );
        let unstated = key(&[("m", 2.0, "kg"), ("g", 9.81, "m/s^2")], "m*g", 19.62, "N");
        assert!(
            check_key(&unstated, "A 2 kg mass hangs at rest.")
                .iter()
                .any(|c| c.id == "givens-stated" && c.outcome == "failed")
        );
        let adds = key(&[("x", 2.0, "m"), ("t", 3.0, "s")], "x + t", 5.0, "m");
        assert!(
            check_key(&adds, "x = 2 m, t = 3 s")
                .iter()
                .any(|c| c.id == "units" && c.outcome == "failed")
        );
    }
    #[test]
    fn answers_match_with_unit_conversion_and_reject_wrong_dimensions() {
        let k = key(&[("d", 12.0, "km"), ("t", 0.5, "h")], "d/t", 24.0, "km/h");
        assert_eq!(check_answer(&k, Some(6.667), "m/s", "").outcome, "passed");
        assert_eq!(check_answer(&k, Some(24.0), "km/h", "").outcome, "passed");
        assert_eq!(check_answer(&k, Some(20.0), "km/h", "").outcome, "failed");
        assert_eq!(check_answer(&k, Some(24.0), "km", "").outcome, "failed");
        assert_eq!(check_answer(&k, Some(24.0), "", "").outcome, "inconclusive");
    }
    #[test]
    fn learner_expressions_are_compared_by_sampling() {
        let k = AnswerKey {
            kind: "expression".into(),
            givens: vec![],
            expression: "2*x*cos(x^2)".into(),
            value: None,
            unit: String::new(),
            tolerance: 0.0,
            variables: vec![DomainVariable {
                name: "x".into(),
                min: -2.0,
                max: 2.0,
            }],
        };
        assert!(check_key(&k, "").iter().all(|c| c.outcome == "passed"));
        assert_eq!(
            check_answer(&k, None, "", "f′(x) = 2x cos(x²)").outcome,
            "passed"
        );
        assert_eq!(check_answer(&k, None, "", "cos(x^2)·2x").outcome, "passed");
        assert_eq!(check_answer(&k, None, "", "2cos(x²)").outcome, "failed");
        assert_eq!(check_answer(&k, None, "", "log(x)").outcome, "inconclusive");
        assert_eq!(check_answer(&k, None, "", "∂∂∂").outcome, "inconclusive");
        let two = parse_expr("xy + 3(x - y)", &["x".into(), "y".into()]).unwrap();
        assert!((two.eval(&[2.0, 5.0]) - 1.0).abs() < 1e-12);
        assert!((parse_expr("-x^2", &["x".into()]).unwrap().eval(&[3.0]) + 9.0).abs() < 1e-12);
        assert!(
            (parse_expr("√(x)+π", &["x".into()]).unwrap().eval(&[4.0])
                - (2.0 + std::f64::consts::PI))
                .abs()
                < 1e-12
        );
    }
    #[test]
    fn hints_that_state_the_answer_are_detected_and_constraints_evaluate() {
        let k = key(&[("d", 12.0, "km"), ("t", 0.5, "h")], "d/t", 24.0, "km/h");
        assert!(hint_leaks(&k, "The speed is 24 km/h."));
        assert!(!hint_leaks(&k, "Divide the distance by the time."));
        let v = constraint_value(
            "a*c - b*b",
            &[("a".into(), 1.0), ("b".into(), 2.0), ("c".into(), 1.0)],
        )
        .unwrap();
        assert!(v < 0.0);
        assert!(parse_expr(&"(".repeat(500), &[]).is_none());
    }
}
