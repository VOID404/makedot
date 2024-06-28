#[derive(Debug)]
pub enum Term<'a> {
    Var(&'a str, &'a str, &'a str),
    Task {
        name: &'a str,
        deps: Vec<&'a str>,
        body: Vec<&'a str>,
    },
}
