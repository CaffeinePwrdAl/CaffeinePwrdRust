fn trim_me(input: &str) -> &str {
    // TODO: Remove whitespace from both ends of a string.
    //let mut s = String::from(input);
    input.trim()
}

fn compose_me(input: &str) -> String {
    // TODO: Add " world!" to the string! There are multiple ways to do this.
    String::from(input) + " world!"
}

fn replace_me(input: &str) -> String {
    // TODO: Replace "cars" in the string with "balloons".
    let idx_result = input.find("cars");
    match idx_result {
        Option::Some(idx) => { 
            let mut output : String = String::from(&input[0..idx]);
            output += "balloons";
            output += &input[(idx+4)..];
            output
        },
        Option::None => String::from(input),
    }
}

fn main() {
    // You can optionally experiment here.
    print!("{}", replace_me("Chasing cars is cool"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_a_string() {
        assert_eq!(trim_me("Hello!     "), "Hello!");
        assert_eq!(trim_me("  What's up!"), "What's up!");
        assert_eq!(trim_me("   Hola!  "), "Hola!");
    }

    #[test]
    fn compose_a_string() {
        assert_eq!(compose_me("Hello"), "Hello world!");
        assert_eq!(compose_me("Goodbye"), "Goodbye world!");
    }

    #[test]
    fn replace_a_string() {
        assert_eq!(
            replace_me("I think cars are cool"),
            "I think balloons are cool",
        );
        assert_eq!(
            replace_me("I love to look at cars"),
            "I love to look at balloons",
        );
    }
}
