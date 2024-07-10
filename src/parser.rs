use nom::{
    branch::alt,
    bytes::complete::{is_a, is_not, tag, take_until},
    character::complete::{alphanumeric1, char, none_of, one_of},
    combinator::{eof, opt, recognize, value},
    error::{context, ParseError, VerboseError},
    multi::{many0, many0_count, many1_count, many_till},
    sequence::{delimited, pair, preceded, terminated, tuple},
    Finish, Parser,
};

use crate::ast::{self, Task, Term, Variable};

pub type ParseErr<'a> = VerboseError<&'a str>;
type ParseResult<'a, O> = nom::IResult<&'a str, O, ParseErr<'a>>;

fn enl(input: &str) -> ParseResult<()> {
    value((), pair(char('\\'), nl)).parse(input)
}

fn hspace0<'a>(tab: bool) -> impl Parser<&'a str, (), ParseErr<'a>> {
    macro_rules! hst {
        () => {};
    }
    let hst = value(
        (),
        many0_count(alt((value((), enl), value((), one_of(" \t"))))),
    );

    let hsnt = value(
        (),
        many0_count(alt((value((), enl), value((), one_of(" \t"))))),
    );
    match tab {
        true => hst,
        false => hsnt,
    }
}

fn hspace1<'a>(tab: bool) -> impl Parser<&'a str, (), ParseErr<'a>> {
    let hst = value(
        (),
        many1_count(alt((value((), enl), value((), one_of(" \t"))))),
    );

    let hsnt = value(
        (),
        many1_count(alt((value((), enl), value((), one_of(" \t"))))),
    );
    match tab {
        true => hst,
        false => hsnt,
    }
}

fn ws0<'a, F, O>(inner: F) -> impl Parser<&'a str, O, ParseErr<'a>>
where
    F: Parser<&'a str, O, ParseErr<'a>>,
{
    terminated(inner, hspace0(true))
}

fn ws1<'a, F, O>(inner: F) -> impl Parser<&'a str, O, ParseErr<'a>>
where
    F: Parser<&'a str, O, ParseErr<'a>>,
{
    terminated(inner, hspace1(true))
}

fn comment<'a>(input: &'a str) -> ParseResult<()> {
    context(
        "comment",
        value(
            (), // Output is thrown away.
            pair(
                ws0(char('#')),
                many0_count(alt((value((), enl), value((), none_of("\n\r"))))),
            ),
        ),
    )
    .parse(input)
}

fn identifier(input: &str) -> ParseResult<&str> {
    let var_start = tag("$(");
    let var_end = char(')');
    let var = recognize(tuple((var_start, is_not(")"), var_end)));
    let idnt = recognize(many1_count(alt((is_a("._-"), alphanumeric1))));
    context("identifier", alt((var, idnt))).parse(input)
}

fn eq(input: &str) -> ParseResult<&str> {
    context("=/?=", alt((tag("="), tag("?=")))).parse(input)
}

fn rest(input: &str) -> ParseResult<&str> {
    context(
        "rest of line",
        recognize(many0_count(alt((
            value((), enl),
            value((), none_of("\n\r#")),
        )))),
    )
    .parse(input)
}

fn var(input: &str) -> ParseResult<(&str, &str, &str)> {
    context(
        "variable",
        tuple((ws0(identifier), ws0(eq), ws0(rest), opt(comment), eol)),
    )
    .map(|(idt, op, val, _, _)| (idt, op, val))
    .parse(input)
}

fn include(input: &str) -> ParseResult<&str> {
    context("include", tuple((tag("include"), rest, opt(comment), eol)))
        .map(|(_, file, _, _)| file)
        .parse(input)
}

fn define(input: &str) -> ParseResult<(&str, &str, &str)> {
    let start = tag("define");
    let end = "endef";
    context("define", tuple((ws0(start), take_until(end), tag(end)))).parse(input)
}

fn task(input: &str) -> ParseResult<(&str, Vec<&str>, Vec<&str>)> {
    context(
        "task",
        tuple((
            // task name
            ws0(identifier),
            ws0(char(':')),
            // task dependencies
            many_till(ws0(identifier), opt(comment).and(eol)).map(|(v, _)| v),
            // task commands
            many0(alt((
                delimited(char('\t'), rest, opt(comment).and(eol)).map(Some),
                value(None, comment.and(eol)),
            )))
            .map(|v| {
                v.into_iter()
                    .filter_map(|v| match v {
                        Some(v) if !v.is_empty() => Some(v),
                        _ => None,
                    })
                    .collect()
            }),
        )),
    )
    .map(|(name, _, deps, cmds)| (name, deps, cmds))
    .parse(input)
}

fn conditional(input: &str) -> ParseResult<(&str, &str, &str)> {
    let starts = alt((tag("ifeq"), tag("ifneq"), tag("ifdef"), tag("ifndef")));
    let end = "endif";
    context(
        "conditional",
        tuple((ws0(starts), take_until(end), tag(end))),
    )
    .parse(input)
}

fn term(input: &str) -> ParseResult<Term> {
    let var = var.map(|(name, op, value)| Term::Variable(Variable { name, op, value }));
    let comment = comment.and(eol).map(|_| Term::Empty);
    let task = task.map(|(name, dependencies, commands)| {
        Term::Task(Task {
            name,
            dependencies,
            commands,
        })
    });
    let conditional = conditional.map(|_| Term::Unimplemented("conditional"));
    let include = include.map(|_| Term::Unimplemented("include"));
    let empty = pair(hspace0(true), eol).map(|_| Term::Empty);
    let define = define.map(|_| Term::Unimplemented("define"));
    context(
        "term",
        alt((empty, define, include, conditional, var, comment, task)),
    )
    .parse(input)
}

fn eol(input: &str) -> ParseResult<()> {
    if input.is_empty() {
        return Ok((input, ()));
    }
    value((), nl).parse(input)
}

#[cfg(target_family = "windows")]
fn nl(input: &str) -> ParseResult<()> {
    value((), tag("\r\n"))(input)
}

#[cfg(not(target_family = "windows"))]
fn nl(input: &str) -> ParseResult<()> {
    value((), tag("\n"))(input)
}

pub struct Makefile;
impl<'a> ast::Parse<'a> for Makefile {
    type Error = ParseErr<'a>;

    fn parse(input: &'a str) -> Result<Vec<Term<'a>>, Self::Error> {
        many_till(ws0(term), eof)(input).finish().map(|v| v.1 .0)
    }
}

#[cfg(test)]
mod test {
    use nom::{error::convert_error, Finish};

    #[test]
    fn test_comment() {
        let cases = [
            ("# hello world!", Ok(("", ()))),
            ("#hello world!", Ok(("", ()))),
            ("# hello world!\n", Ok(("\n", ()))),
            ("# hello world!\r\n", Ok(("\r\n", ()))),
            ("# hello world! # comment", Ok(("", ()))),
            ("# hello world! \\\n comment", Ok(("", ()))),
            ("# hello world! \n", Ok(("\n", ()))),
            ("# hello world! \r\n", Ok(("\r\n", ()))),
            ("# hello world! \n# comment", Ok(("\n# comment", ()))),
            ("# hello world! \r\n# comment", Ok(("\r\n# comment", ()))),
        ];

        for (i, (input, expected)) in cases.into_iter().enumerate() {
            let result = super::comment(input);
            assert_eq!(result, expected, "case {:02}, input: {:?}", i, input);
        }
    }

    #[test]
    fn test_vars() {
        let cases = [
            ("foo=bar", Ok(("", ("foo", "=", "bar")))),
            ("var = value", Ok(("", ("var", "=", "value")))),
            ("_var=123", Ok(("", ("_var", "=", "123")))),
            ("VAR=Hello World!", Ok(("", ("VAR", "=", "Hello World!")))),
            ("var1=123 var2=456", Ok(("", ("var1", "=", "123 var2=456")))),
            ("var1=123\t\\\n456", Ok(("", ("var1", "=", "123\t\\\n456")))),
            ("var1=123\nvar2=456", Ok(("var2=456", ("var1", "=", "123")))), // Newline separates variables
            ("var1=123 # comment", Ok(("", ("var1", "=", "123 ")))), // Comment after variable
            (
                "var1=123 \\\n #comment",
                Ok(("", ("var1", "=", "123 \\\n "))),
            ), // Comment after variable with line continuation
            (
                "var1=123\n# comment",
                Ok(("# comment", ("var1", "=", "123"))),
            ), // Comment after newline
        ];

        for (i, (input, expected)) in cases.into_iter().enumerate() {
            let result = super::var(input).finish();
            // assert_eq!(result, expected, "case {:02}, input: {:?}", i, input);
            match (result, expected) {
                (Err(e), Ok(_)) => {
                    let e = convert_error(input, e);
                    panic!(
                        "case {:02}, input: {:?}, error:\n{}\n---------",
                        i, input, e
                    );
                }
                (v1, v2) => assert_eq!(v1, v2, "case {:02}, input: {:?}", i, input),
            }
        }
    }
}
