// Rust methods and impl blocks

struct Calculator {
    value: i32,
}

impl Calculator {
    fn new() -> Self {
        Calculator { value: 0 }
    }

    fn add(&mut self, x: i32) {
        self.value += x;
    }

    fn subtract(&mut self, x: i32) {
        self.value -= x;
    }

    fn multiply(&mut self, x: i32) {
        self.value *= x;
    }

    fn get_value(&self) -> i32 {
        self.value
    }

    fn reset(&mut self) {
        self.value = 0;
    }

    fn conditional_add(&mut self, x: i32) {
        if x > 0 {
            self.value += x;
        } else {
            self.value -= x.abs();
        }
    }
}

struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn new(x: i32, y: i32) -> Self {
        Point { x, y }
    }

    fn distance_from_origin(&self) -> f64 {
        let sum = (self.x * self.x + self.y * self.y) as f64;
        sum.sqrt()
    }

    fn is_positive(&self) -> bool {
        self.x > 0 && self.y > 0
    }

    fn quadrant(&self) -> &'static str {
        match (self.x > 0, self.y > 0) {
            (true, true) => "first",
            (false, true) => "second",
            (false, false) => "third",
            (true, false) => "fourth",
        }
    }
}

trait Drawable {
    fn draw(&self);
}

impl Drawable for Point {
    fn draw(&self) {
        println!("Point at ({}, {})", self.x, self.y);
    }
}

struct Rectangle {
    width: i32,
    height: i32,
}

impl Rectangle {
    fn area(&self) -> i32 {
        self.width * self.height
    }

    fn is_square(&self) -> bool {
        self.width == self.height
    }
}

impl Drawable for Rectangle {
    fn draw(&self) {
        for _ in 0..self.height {
            for _ in 0..self.width {
                print!("*");
            }
            println!();
        }
    }
}
