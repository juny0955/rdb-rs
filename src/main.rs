use std::io::{Write, stdin, stdout};

fn main() {
    let mut input = String::new();

    loop {
        print!("rdb> ");
        let _ = stdout().flush();

        input.clear();
        match stdin().read_line(&mut input) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                println!("{e}");
                break;
            }
        }

        let input = input.trim();
        if input == "\\q" {
            break;
        }
    }
}
