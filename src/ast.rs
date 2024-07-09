#[derive(Debug)]
pub struct Task<'a> {
    pub name: &'a str,
    pub dependencies: Vec<&'a str>,
    pub commands: Vec<&'a str>,
}

#[derive(Debug)]
pub struct Variable<'a> {
    pub name: &'a str,
    pub op: &'a str,
    pub value: &'a str,
}

#[derive(Debug)]
pub enum Term<'a> {
    Task(Task<'a>),
    Variable(Variable<'a>),
    Empty,
    Unimplemented(&'static str),
}

pub trait Parse<'a> {
    type Error;
    fn parse(input: &'a str) -> Result<Vec<Term>, Self::Error>;
}
