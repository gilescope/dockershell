#![deny(
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]
#![allow(unused_assignments,stable_features)]
#![feature(pin)]
#![feature(futures_api)]
#![feature(await_macro)]
#![feature(async_await)]

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::pin::Pin;

use dockworker::*;
use futures::executor::block_on;
use futures::future::FutureExt;
use futures::Future;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use tar::Builder;

mod exec;

use self::exec::{execute_command, ExecResults};

type Result<T> = std::result::Result<T, ()>;

/// Future Image Name. Resolves once Docker has built the image.
type FutureImage = Pin<Box<Future<Output = Box<String>>>>;

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub debug: bool,

    /// Enables ascii colors and one day maybe Vi to work.
    pub tty: bool,

    /// Initial commands to run.
    pub lines: Vec<Vec<String>>,
    pub image_name: String,
    pub pwd: String,
    pub shell: String,
}

impl State {
    pub fn test() -> State {
        State {
            tty: false,
            debug: true,
            pwd: "/bin".to_owned(),
            ..State::default()
        }
    }
}

impl Default for State {
    fn default() -> Self {
        State {
            debug: false,
            tty: true,
            lines: vec![vec!["FROM".to_owned(), "alpine:edge".to_owned()]],
            image_name: "alpine:edge".to_owned(),
            pwd: String::new(),
            shell: "/bin/sh".to_owned(),
        }
    }
}

pub trait ExecListener {
    fn command_run(&mut self, line: &str, state: &State, line_result: Result<&LineResult>);
}

pub struct NoOpListener {}

impl ExecListener for NoOpListener {
    fn command_run(&mut self, _line: &str, _state: &State, _line_result: Result<&LineResult>) {}
}

pub fn interpreter_loop_from_stdin(initial_state: State) -> Result<()> {
    let mut rl = ReadLinePrompt {
        editor: Editor::<()>::new(),
    };
    if rl.editor.load_history("Dockerfile.dockershell").is_err() {
        println!("No previous history.");
    }

    let res = interpreter_loop(initial_state, &mut rl, &mut NoOpListener {});

    rl.editor.save_history("Dockerfile.dockershell").unwrap();
    res
}

pub fn interpreter_loop_from_file(initial_state: State, visitor: &mut ExecListener) -> Result<()> {
    let mut rl = FilePrompt {
        lines: initial_state
            .lines
            .iter()
            .skip(1)
            .map(|s| s.join(" "))
            .collect(),
    };

    interpreter_loop(initial_state, &mut rl, visitor)
}

pub trait ReadPrompt {
    fn read_line(&mut self, prompt: &str) -> std::result::Result<String, ReadlineError>;
    fn add_history_entry(&mut self, val: &str);
}

struct ReadLinePrompt {
    editor: Editor<()>,
}

impl ReadPrompt for ReadLinePrompt {
    fn read_line(&mut self, prompt: &str) -> std::result::Result<String, ReadlineError> {
        self.editor.readline(prompt)
    }

    fn add_history_entry(&mut self, val: &str) {
        self.editor.add_history_entry(val);
    }
}

struct FilePrompt {
    lines: Vec<String>,
}

impl ReadPrompt for FilePrompt {
    fn read_line(&mut self, _prompt: &str) -> std::result::Result<String, ReadlineError> {
        if self.lines.is_empty() {
            return Err(ReadlineError::Eof);
        }
        let res = self.lines.remove(0);
        Ok(res)
    }

    fn add_history_entry(&mut self, _val: &str) {}
}

/// start from a known from image. FROM=
/// create container.
/// wait for line of input.
/// create continer from previous image.
/// run command
/// containerNext = container.commitImage
/// show results of command
pub fn interpreter_loop(
    initial_state: State,
    rl: &mut ReadPrompt,
    visitor: &mut ExecListener,
) -> Result<()> {
    let docker = Docker::connect_with_defaults().unwrap();

    block_on(
        async {
            let mut last_image: Option<FutureImage> = None;
            let mut state = initial_state.clone();
            state.lines.clear();
            state.lines.push(initial_state.lines[0].clone());

            assert_eq!(initial_state.lines[0][0], "FROM");
            state.image_name = initial_state.lines[0][1].clone();

            state.lines.push(vec!["RUN".to_owned(), ("pwd").to_owned()]);
            let exec_results = execute_command(&docker, &state).unwrap();
            state.lines.pop();
            state.pwd = exec_results.output.trim().to_owned();
            let mut state_stack = vec![state];

            loop {
                let prompt = &(state_stack.last().unwrap().pwd.clone() + " ");
                std::io::stdout().lock().flush().unwrap();
                let readline = rl.read_line(prompt);
                match readline {
                    Ok(line) => {
                        rl.add_history_entry(line.as_ref());

                        if let Some(future) = last_image {
                            let name = *await!(future);

                            if docker.history_image(&name).is_err() {
                                //Roll back to previous state....
                                last_image = None;
                                let bad_state = state_stack.pop().unwrap();
                                let popped = bad_state.lines.last().unwrap();
                                println!("Could not re-run prev command: {:?}", popped);
                            } else {
                                state_stack.last_mut().unwrap().image_name = name;
                            }
                        }

                        last_image = None;

                        let result = parse_line(&line, &state_stack.last().unwrap(), &docker);
                        visitor.command_run(
                            &line,
                            &state_stack.last().unwrap(),
                            match &result {
                                Ok((line_result, _)) => Ok(line_result),
                                Err(()) => Err(()),
                            },
                        );
                        match result {
                            Ok((LineResult::NoOp(_output), None)) => {}
                            Ok((LineResult::State(new_state, _output), fut)) => {
                                last_image = fut;
                                state_stack.push(new_state);
                            }
                            Ok((LineResult::Exit, None)) => {
                                break;
                            }
                            Ok((_, _)) => unimplemented!(),
                            Err(()) => {}
                        }
                    }
                    Err(ReadlineError::Interrupted) => {
                        println!("CTRL-C");
                        break;
                    }
                    Err(ReadlineError::Eof) => {
                        println!("CTRL-D");
                        break;
                    }
                    Err(err) => {
                        println!("Error: {:?}", err);
                        break;
                    }
                }
            }
            Ok(())
        },
    )
}

fn print_dockerfile(lines: &[Vec<String>]) {
    for l in lines.iter() {
        println!("{}", l.join(" "));
    }
}

fn print_layers(lines: &[Vec<String>]) {
    for (i, l) in lines.iter().enumerate() {
        println!("{}: {:?}", i, l);
    }
}

#[derive(Debug, PartialEq)]
pub enum LineResult {
    Exit,
    NoOp(String), // E.g. print state...
    State(State, String),
}

pub fn parse_line(
    mut line: &str,
    state: &State,
    docker: &Docker,
) -> Result<(LineResult, Option<FutureImage>)> {
    assert_eq!(state.lines[0][0], "FROM");
    line = line.trim();
    match line {
        "" => Ok((LineResult::NoOp(String::new()), None)),
        "exit" => {
            println!("Dockerfile of session:");
            print_dockerfile(&state.lines);
            Ok((LineResult::Exit, None))
        }
        "debug" => {
            let mut state = state.clone();
            state.debug = !state.debug;
            Ok((LineResult::State(state, String::new()), None))
        }
        // TODO: undo 3 should remove 3rd item.
        // Replay breaks - fix then type continue.
        "undo" => {
            let mut state = state.clone();
            let item = state.lines.pop();
            println!("Undone: {:?}", item);
            Ok((LineResult::State(state, String::new()), None))
        }
        "layers" => {
            print_layers(&state.lines);
            Ok((LineResult::NoOp(String::new()), None))
        }
        "image" => {
            println!("image name {}", &state.image_name);
            Ok((LineResult::NoOp(String::new()), None))
        }
        _ => {
            let initial_state = state;
            let mut state = initial_state.clone();
            if line.starts_with("cd ") || line == "cd" {
                state.lines.push(vec![
                    "RUN".to_owned(),
                    (line.to_string() + " ; pwd").to_owned(),
                ]);
                let exec_results = execute_command(&docker, &state)?;
                state.lines.pop();
                //what do you tell it to build
                if state.debug {
                    println!("DIR SET TO {:?}", exec_results.output.trim());
                }
                let new_pwd = exec_results.output.trim().to_owned();
                if new_pwd.lines().count() == 1 {
                    // Inefficient to have two WORKDIR statements in a row...
                    if state.lines.last().unwrap()[0] == "WORKDIR" {
                        state.lines.pop();
                    }

                    state.pwd = new_pwd;
                    state
                        .lines
                        .push(vec!["WORKDIR".to_owned(), state.pwd.clone()]);
                    let image_name = build_image(
                        exec_results.container_name,
                        state.lines.clone(),
                        state.debug,
                    )
                        .boxed();
                    state.image_name = "Pending".to_owned(); //todo enum.
                    Ok((
                        LineResult::State(state, exec_results.output),
                        Some(image_name),
                    ))
                } else {
                    Err(())
                } //TODO return exec results..
            } else {
                state.lines.push(vec!["RUN".to_owned(), line.to_owned()]);
                let exec_result = execute_command(&docker, &state);

                match exec_result {
                    Ok(ExecResults {
                        state_change: true,
                        container_name,
                        output,
                        ..
                    }) => {
                        let image_name =
                            build_image(container_name, state.lines.clone(), state.debug).boxed();

                        Ok((LineResult::State(state, output), Some(image_name)))
                    }
                    Ok(ExecResults {
                        state_change: false,
                        output,
                        ..
                    }) => {
                        let removed = state.lines.remove(state.lines.len() - 1);
                        if state.debug {
                            println!("No state change, removed {:?}. State={:?}", removed, state);
                        }
                        Ok((LineResult::NoOp(output), None))
                    }
                    Err(()) => Err(()),
                }
            }
        }
    }
}

async fn build_image(
    image_name: String,
    command_lines: Vec<Vec<String>>,
    debug: bool,
) -> Box<String> {
    assert_eq!(command_lines[0][0], "FROM");
    if debug {
        println!("building img {} as {:?}", &image_name, &command_lines)
    }
    let docker = Docker::connect_with_defaults().unwrap();
    {
        let mut dockerfile = File::create("Dockerfile").unwrap();
        let lines: Vec<String> = command_lines.iter().map(|args| args.join(" ")).collect();
        dockerfile.write_all(lines.join("\n").as_bytes()).unwrap();
    }
    // Create tar file
    {
        let tar_file = File::create("image.tar").unwrap();
        let mut a = Builder::new(tar_file);
        a.append_path("Dockerfile").unwrap();
    }
    let options = ContainerBuildOptions {
        t: vec![image_name.to_owned()],
        ..ContainerBuildOptions::default()
    };
    let res = docker.build_image(options, Path::new("image.tar")).unwrap();

    for line in BufReader::new(res).lines() {
        let buf = line.unwrap();
        if debug {
            println!("{}", &buf);
        }
    }
    if debug {
        println!("built image {}", &image_name);
    }

    Box::new(image_name.to_owned())
}

mod tests {
    #[test]
    fn initial_command() {
        let docker = dockworker::Docker::connect_with_defaults().unwrap();
        let state = super::State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec![
                    "RUN".to_owned(),
                    "/bin/echo".to_owned(),
                    "Hello World".to_owned(),
                ],
            ],
            debug: true,
            tty: false,
            image_name: "alpine:edge".to_owned(),
            pwd: "/bin".to_owned(),
            shell: "/bin/sh".to_owned(),
        };

        let exec_results: super::ExecResults = super::execute_command(&docker, &state).unwrap();

        println!("{}", exec_results.output);
        assert!(exec_results.output.contains("Hello World"));
    }
}
