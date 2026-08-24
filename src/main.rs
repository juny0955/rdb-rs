use std::{
    io::{Write, stdin, stdout},
    path::Path,
};

use rdb_rs::{
    database::Database,
    parser::{Parser, lexer::Lexer},
};

fn main() {
    let mut database = match Database::open(Path::new("./data"), "default") {
        Ok(database) => database,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

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

        let tokens = match Lexer::new(input).tokenize() {
            Ok(tokens) => tokens,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };

        let statement = match Parser::new(tokens).parse() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };

        match database.execute(&statement) {
            Ok(result) => println!("{result:?}"),
            Err(e) => eprintln!("{e}"),
        }
    }
}
