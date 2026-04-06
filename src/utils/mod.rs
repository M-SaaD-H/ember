pub fn parse_int(s: &String) -> i32 {
    s.trim().parse::<i32>().unwrap_or(0)
}