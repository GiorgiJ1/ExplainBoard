mod diagram;
mod ollama;

use diagram::{Diagram, Element};
use std::io::{self, Write};

#[tokio::main]
async fn main() {
    println!("ExplainBoard — Day 1 (terminal pipeline test)");
    println!("Type something to explain, or type 'exit' to quit.\n");

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            println!("Failed to read input. Try again.");
            continue;
        }
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("exit") {
            println!("Goodbye!");
            break;
        }

        println!("\nGenerating diagram...\n");

        match ollama::generate_diagram(input).await {
            Ok(diagram) => print_diagram(&diagram),
            Err(e) => println!("Something went wrong:\n{}\n", e),
        }

        println!();
    }
}

fn print_diagram(diagram: &Diagram) {
    println!("Title: {}\n", diagram.title);

    for element in &diagram.elements {
        match element {
            Element::Box { id, x, y, width, height, text } => {
                println!("BOX: {}", text);
                println!("  id: {}", id);
                println!("  position: {}, {}", x, y);
                println!("  size: {} x {}\n", width, height);
            }
            Element::Circle { id, x, y, radius, text } => {
                println!("CIRCLE: {}", text);
                println!("  id: {}", id);
                println!("  position: {}, {}", x, y);
                println!("  radius: {}\n", radius);
            }
            Element::Text { id, x, y, text } => {
                println!("TEXT: {}", text);
                println!("  id: {}", id);
                println!("  position: {}, {}\n", x, y);
            }
            Element::Arrow { from, to, text } => {
                if text.is_empty() {
                    println!("ARROW: {} -> {}\n", from, to);
                } else {
                    println!("ARROW: {} -> {}", from, to);
                    println!("  label: {}\n", text);
                }
            }
        }
    }
}