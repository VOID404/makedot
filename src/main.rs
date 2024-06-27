use post::split_kind;

fn main() {
    let sample = include_str!("../Makefile");
    let (_, terms) = grammar::makefile(&sample).unwrap();
    let (tasks, vars) = split_kind(terms);
    for (k, v) in tasks.iter() {
        println!("{}: {}\n{}", k, v.deps.join(", "), v.body)
    }
    // println!("{:#?}", tasks);
}

mod post {
    use std::collections::HashMap;

    use crate::grammar::{self, Term};

    #[derive(Debug, PartialEq, Eq)]
    pub struct Variable<'a> {
        pub name: &'a str,
        pub value: &'a str,
        pub eq: &'a str,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub struct Task<'a> {
        pub phony: bool,
        pub name: &'a str,
        pub deps: Vec<&'a str>,
        pub body: &'a str,
    }

    fn convert_task<'a>(phonies: &Vec<&str>, task: grammar::Task<'a>) -> Task<'a> {
        assert!(task.name != ".PHONY");
        Task {
            phony: phonies.contains(&task.name),
            name: task.name,
            deps: task.deps,
            body: task.body,
        }
    }

    fn convert_variable(var: grammar::Variable) -> Variable {
        Variable {
            name: var.name,
            value: var.value,
            eq: var.eq,
        }
    }

    pub fn split_kind<'a>(
        terms: Vec<grammar::Term<'a>>,
    ) -> (HashMap<&'a str, Task>, HashMap<&'a str, Variable>) {
        let mut tasks = HashMap::new();
        let mut vars = HashMap::new();
        let phonies = terms
            .iter()
            .filter_map(|t| match t {
                Term::Task(task) if task.name == ".PHONY" => Some(task.deps.clone()),
                _ => None,
            })
            .flatten()
            .collect();

        for term in terms {
            match term {
                Term::Variable(v) => {
                    vars.insert(v.name, convert_variable(v));
                }
                Term::Task(t) if t.name == ".PHONY" => {}
                Term::Task(t) if tasks.contains_key(t.name) => {
                    let out: &mut Task = tasks.get_mut(t.name).unwrap();
                    out.deps.append(&mut t.deps.clone());
                    if !t.body.is_empty() {
                        out.body = t.body;
                    }
                }
                // tasks[t.name] = convert_task(phonies, t)
                Term::Task(t) => {
                    tasks.insert(t.name, convert_task(&phonies, t));
                }
            }
        }

        (tasks, vars)
    }
}

mod grammar {
    use nom::{
        branch::alt,
        bytes::complete::{is_a, is_not, tag, take_until},
        character::complete::{alpha1, alphanumeric1, multispace0, newline, one_of},
        combinator::{recognize, value},
        error::ParseError,
        multi::{many0, many0_count, many1, separated_list0},
        sequence::{delimited, pair, preceded, terminated, tuple},
        IResult, Parser,
    };

    #[derive(Debug, PartialEq, Eq)]
    pub struct Variable<'a> {
        pub name: &'a str,
        pub value: &'a str,
        pub eq: &'a str,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub struct Task<'a> {
        pub name: &'a str,
        pub deps: Vec<&'a str>,
        pub body: &'a str,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum Term<'a> {
        Variable(Variable<'a>),
        Task(Task<'a>),
    }

    fn identifier(input: &str) -> IResult<&str, &str> {
        let allowed_symbols = b"asd";
        recognize(pair(
            alt((alpha1, recognize(one_of("_.-")))),
            many0_count(alt((alphanumeric1, recognize(one_of("_.-"))))),
        ))
        .parse(input)
    }

    fn comment<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&str, &str, E> {
        recognize(pair(tag("#"), is_not("\n\r"))).parse(input)
    }

    fn ws<'a, F, O, E>(inner: F) -> impl Parser<&'a str, O, E>
    where
        F: Parser<&'a str, O, E>,
        E: ParseError<&'a str>,
    {
        delimited(multispace0, inner, multispace0)
    }

    fn line<'a, F, O, E>(inner: F) -> impl Parser<&'a str, O, E>
    where
        F: Parser<&'a str, O, E>,
        E: ParseError<&'a str>,
    {
        terminated(inner, tuple((take_until("\n"), tag("\n"))))
    }

    fn wsenl<'a, F, O, E>(inner: F) -> impl Parser<&'a str, O, E>
    where
        F: Parser<&'a str, O, E>,
        E: ParseError<&'a str>,
    {
        let wsn = value((), many0(alt((tag(" "), tag("\t"), tag("\\\n")))));
        preceded(wsn, inner)
    }

    fn ignore<'a, F, O, E>(inner: F) -> impl Parser<&'a str, O, E>
    where
        F: Parser<&'a str, O, E>,
        E: ParseError<&'a str>,
    {
        delimited(multispace0, inner, ws(comment))
    }

    fn variable(input: &str) -> IResult<&str, Term> {
        let eq = alt((tag("="), tag("?=")));
        tuple((identifier, ws(eq), is_not("#\n\r")))
            .map(|(name, eq, value)| Term::Variable(Variable { name, value, eq }))
            .parse(input)
    }

    fn task(input: &str) -> IResult<&str, Term> {
        let deps = line(separated_list0(wsenl(tag(",")), identifier));
        let indent = many1(alt((tag("\t"), tag("  "))));
        let body = recognize(many0(line(preceded(indent, is_not("\n")))));
        tuple((identifier, ws(tag(":")), deps, body))
            .map(|(name, _, deps, body)| Term::Task(Task { name, deps, body }))
            .parse(input)
    }

    fn term(input: &str) -> IResult<&str, Term> {
        let (mut rest, out) = ws(alt((variable, task))).parse(input)?;
        if let Ok((r, _)) = comment::<nom::error::Error<&str>>(rest) {
            rest = r;
        };
        Ok((rest, out))
    }

    pub fn makefile(input: &str) -> IResult<&str, Vec<Term>> {
        many0(term).parse(input)
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn test_identifier() {
            assert_eq!(identifier("foo"), Ok(("", "foo")));
            assert_eq!(identifier("foo_bar"), Ok(("", "foo_bar")));
            assert_eq!(identifier("foo123"), Ok(("", "foo123")));
            assert_eq!(identifier("_foo"), Ok(("", "_foo")));
            assert_eq!(identifier("_foo_bar"), Ok(("", "_foo_bar")));
            assert_eq!(identifier("_foo123"), Ok(("", "_foo123")));
        }
    }
}
