pub fn display_success(message: &str) {
    println!("Success: {}", message);
}

pub fn display_error(message: &str) {
    eprintln!("Error: {}", message);
}

pub fn display_warning(message: &str) {
    println!("Warning: {}", message);
}

pub fn display_info(message: &str) {
    println!("Info: {}", message);
}
