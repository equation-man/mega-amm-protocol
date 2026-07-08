mod protocol_instructions;
mod protocol_cli;

use protocol_cli::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // First argument at offset 0 is the file name. Offset 1 is our action argument.
    let action = args.get(1).expect("Incorrect action passed");
    let target = args.get(2).expect("Incorrect target token");
    // Should be changed from &String to u64
    let amount = args.get(3).expect("Incorrect amount set");
    process(action, target, amount);
}
