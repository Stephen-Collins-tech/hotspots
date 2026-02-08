// Match expressions in Rust

fn simple_match(x: i32) -> &'static str {
    match x {
        0 => "zero",
        1 => "one",
        _ => "other",
    }
}

fn match_with_guards(x: i32) -> &'static str {
    match x {
        n if n < 0 => "negative",
        0 => "zero",
        n if n > 0 && n < 10 => "small positive",
        _ => "large positive",
    }
}

fn match_with_ranges(x: i32) -> &'static str {
    match x {
        0..=10 => "small",
        11..=100 => "medium",
        101..=1000 => "large",
        _ => "very large",
    }
}

enum Status {
    Active,
    Inactive,
    Pending,
}

fn match_enum(status: Status) -> &'static str {
    match status {
        Status::Active => "active",
        Status::Inactive => "inactive",
        Status::Pending => "pending",
    }
}

fn nested_match(x: i32, y: i32) -> &'static str {
    match x {
        0 => match y {
            0 => "both zero",
            _ => "x zero",
        },
        _ => match y {
            0 => "y zero",
            _ => "neither zero",
        },
    }
}

fn match_option(opt: Option<i32>) -> i32 {
    match opt {
        Some(n) if n > 0 => n * 2,
        Some(n) => n,
        None => 0,
    }
}

fn match_result(res: Result<i32, String>) -> i32 {
    match res {
        Ok(n) => n,
        Err(_) => -1,
    }
}
