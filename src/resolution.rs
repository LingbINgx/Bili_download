use std::collections::HashMap;

pub fn qn(s: &str) -> &str {
    let hash: HashMap<&str, &str> = [
        ("HDR", "125"),
        ("4K", "120"),
        ("1080P+", "112"),
        ("1080P60", "116"),
        ("1080P", "80"),
        ("720P", "64"),
        ("480P", "32"),
        ("360P", "16"),
    ]
    .iter()
    .cloned()
    .collect();
    hash.get(s).map(|&v| v).unwrap_or("")
}

pub fn fnval(s: &str) -> &str {
    let hash: HashMap<&str, &str> = [("HDR", "80"), ("4K", "144")].iter().cloned().collect();
    hash.get(s).map(|&v| v).unwrap_or("16")
}

pub fn rsl(s: &str) -> &str {
    let hash: HashMap<&str, &str> = [
        ("125", "HDR"),
        ("120", "4K"),
        ("112", "1080P+"),
        ("116", "1080P60"),
        ("80", "1080P"),
        ("64", "720P"),
        ("32", "480P"),
        ("16", "360P"),
    ]
    .iter()
    .cloned()
    .collect();
    hash.get(s).map(|&v| v).unwrap_or("")
}

#[test]
fn x() {
    let s = "HDR";
    println!("{}", qn(s));
    println!("{}", fnval(s));
}
