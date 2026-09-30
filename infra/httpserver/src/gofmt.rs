//! Go-compatible number/value formatting (`fmt` `%v` and `encoding/json`).
use serde_json::Value;

/// Shortest round-trip decimal digits and decimal exponent of `f` (finite,
/// non-zero, positive): value = 0.DIGITS * 10^dp, mirroring strconv's decimal.
fn shortest(f: f64) -> (String, i32) {
    let s = format!("{:e}", f);
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let exp: i32 = exp.parse().unwrap_or(0);
    (digits, exp + 1)
}

fn fmt_e(digits: &str, dp: i32) -> String {
    let mut s = String::new();
    s.push_str(&digits[..1]);
    if digits.len() > 1 {
        s.push('.');
        s.push_str(&digits[1..]);
    }
    let exp = dp - 1;
    s.push('e');
    s.push(if exp < 0 { '-' } else { '+' });
    let a = exp.abs();
    if a < 10 {
        s.push('0');
    }
    s.push_str(&a.to_string());
    s
}

fn fmt_f(digits: &str, dp: i32) -> String {
    let nd = digits.len() as i32;
    if dp <= 0 {
        format!("0.{}{}", "0".repeat((-dp) as usize), digits)
    } else if dp >= nd {
        format!("{}{}", digits, "0".repeat((dp - nd) as usize))
    } else {
        format!("{}.{}", &digits[..dp as usize], &digits[dp as usize..])
    }
}

/// Go's `%v` for a float64 (`strconv.FormatFloat(f, 'g', -1, 64)`).
pub fn fmt_g(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf".into() } else { "-Inf".into() };
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let neg = f < 0.0;
    let (digits, dp) = shortest(f.abs());
    let exp = dp - 1;
    let body = if !(-4..6).contains(&exp) {
        fmt_e(&digits, dp)
    } else {
        fmt_f(&digits, dp)
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// Go's `encoding/json` float64 encoding.
pub fn json_float(f: f64) -> String {
    if !f.is_finite() {
        return "null".into();
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let abs = f.abs();
    if !(1e-6..1e21).contains(&abs) {
        let neg = f < 0.0;
        let (digits, dp) = shortest(abs);
        let mut s = fmt_e(&digits, dp);
        // Clean up e-09 to e-9.
        let n = s.len();
        let b = s.as_bytes();
        if n >= 4 && b[n - 4] == b'e' && b[n - 3] == b'-' && b[n - 2] == b'0' {
            let last = s.pop().unwrap_or('0');
            s.pop();
            s.push(last);
        }
        if neg {
            s.insert(0, '-');
        }
        s
    } else {
        format!("{}", f)
    }
}

fn num_f64(n: &serde_json::Number) -> f64 {
    n.as_f64().unwrap_or(0.0)
}

/// Go's `fmt.Sprint` of a value decoded from JSON into `any`.
pub fn sprint_v(v: &Value) -> String {
    match v {
        Value::Null => "<nil>".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => fmt_g(num_f64(n)),
        Value::String(s) => s.clone(),
        Value::Array(a) => {
            let parts: Vec<String> = a.iter().map(sprint_v).collect();
            format!("[{}]", parts.join(" "))
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys.iter().map(|k| format!("{}:{}", k, sprint_v(&m[*k]))).collect();
            format!("map[{}]", parts.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format() {
        assert_eq!(fmt_g(3.0), "3");
        assert_eq!(fmt_g(3.5), "3.5");
        assert_eq!(fmt_g(1e6), "1e+06");
        assert_eq!(fmt_g(100000.0), "100000");
        assert_eq!(fmt_g(123456789.0), "1.23456789e+08");
        assert_eq!(fmt_g(0.0001), "0.0001");
        assert_eq!(fmt_g(0.00001), "1e-05");
        assert_eq!(fmt_g(-2.5), "-2.5");
    }

    #[test]
    fn json_format() {
        assert_eq!(json_float(1e6), "1000000");
        assert_eq!(json_float(1e21), "1e+21");
        assert_eq!(json_float(1e-7), "1e-7");
        assert_eq!(json_float(1.5), "1.5");
        assert_eq!(json_float(3.0), "3");
    }
}
