use dockershell::*;
use clap::{App,Arg};
use std::fs::File;
use std::io::BufReader;
use std::io::BufRead;
use std::result::Result;

fn main() -> Result<(), std::io::Error> {
    let matches = App::new("dockershell")
        .version("0.1")
        .about("A shell with undo.")
        .arg(
        Arg::with_name("image")
            .short("i")
            .value_name("image_name")
            .help("Docker image name or id to use as a base image")
            .required(false)
            .takes_value(true),
       ).arg(
        Arg::with_name("dockerfile")
            .short("f")
            .help("Dockerfile of instructions to pre-run")
            .required(false)
            .multiple(true),
    ).get_matches();

    let (lines, image_name) = if let Some(dockerfile) = matches.value_of("dockerfile") {
        let file = File::open(dockerfile)?;
        let l = parse_dockerfile(BufReader::new(file).lines());
        let i = l[0][1].clone();
        (l, i)
    } else {
        let image_name = matches.value_of("image").unwrap_or("alpine:edge").to_owned();
        (vec![vec!["FROM".to_owned(), image_name.clone()]], image_name)
    };

    let state = State {
        debug: false,
        tty: true,
        lines,
        image_name,
        pwd: String::new(),
        shell: "/bin/sh".to_owned(),
    };

    interpreter_loop_from_stdin(state).unwrap();
    Ok(())
}

fn parse_dockerfile<T>(lines: T) -> Vec<Vec<String>>
    where T: IntoIterator<Item=Result<String, std::io::Error>> {

    let mut results = vec![];

    for line in lines {
        // We cheat - not actually splitting into more than two parts...
        let raw = line.unwrap();
        let parts : Vec<_> = raw.split(" ").collect();
        results.push(vec![ parts[0].to_owned(), raw[parts[0].len()..].to_owned() ]);
    }

    results
}