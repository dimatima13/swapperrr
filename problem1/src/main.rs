use problem1::pad_numbers;

fn main() {
    let examples = vec![
        ("James Bond 7", 3),
        ("PI=3.14", 2),
        ("It's 3:13pm", 2),
        ("It's 12:13pm", 2),
        ("99UR1337", 6),
    ];

    for (input, pad_length) in examples {
        let result = pad_numbers(input, pad_length);
        println!("Input: \"{}\", Pad: {} -> Output: \"{}\"", input, pad_length, result);
    }
}