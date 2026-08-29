/// The sample project every runtime ships: something the conformance test can build and test.
pub fn greet(name: &str) -> String {
    format!("hello {name}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn greets() {
        assert_eq!(super::greet("rig"), "hello rig");
    }
}
