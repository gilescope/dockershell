#![deny(
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]
#![allow(unused_imports)]
#![feature(pin)]
#![feature(futures_api)]
#![feature(await_macro)]
#![feature(async_await)]

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::pin::Pin;

use futures::executor::block_on;
use futures::future::FutureExt;
use futures::Future;

use termion::raw::IntoRawMode;

use rustyline::error::ReadlineError;
use rustyline::Editor;

use dockworker::container::*;
use dockworker::*;

use rand::Rng;

use tar::Builder;

type Result<T> = std::result::Result<T, ()>;

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub debug: bool,

    /// Enables ascii colors and one day maybe Vi to work.
    pub tty: bool,

    /// Should read from stdin for extra commands.
    pub interactive: bool,

    /// Initial commands to run.
    pub lines: Vec<Vec<String>>,
    pub image_name: String,
    pub pwd: String,
    pub shell: String,
}

impl State {
    pub fn test() -> State {
        State {
            interactive: false,
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
            interactive: true,
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

///
/// start from a known from image. FROM=
/// create container.
/// wait for line of input.
/// create continer from previous image.
/// run command
/// containerNext = container.commitImage
/// show results of command
///
pub fn interpreter_loop(initial_state: State, visitor: &mut ExecListener) -> Result<()> {
    let docker = Docker::connect_with_defaults().unwrap();

    block_on(
        async {
            let mut rl = Editor::<()>::new();
            if rl.load_history("Dockerfile.dockershell").is_err() {
                println!("No previous history.");
            }
            let mut last_image: Option<Pin<Box<Future<Output = Box<String>>>>> = None;
            let mut state = initial_state.clone();
            state.lines.clear();
            state.lines.push(initial_state.lines[0].clone());

            assert_eq!(initial_state.lines[0][0], "FROM");
            state.image_name = initial_state.lines[0][1].clone();

            for line in initial_state.lines.iter().skip(1) {
                // skip FROM
                let mut next_state = state.clone();
                next_state.image_name = match last_image {
                    None => next_state.image_name,
                    Some(future) => *await!(future),
                };
                last_image = None;
                let line = line.join(" ");
                let result = parse_line(&line, &next_state, &docker);

                visitor.command_run(
                    &line,
                    &next_state,
                    match &result {
                        Ok((line_result, _)) => Ok(line_result),
                        Err(()) => Err(()),
                    },
                );

                match result {
                    Ok((LineResult::NoOp, None)) => {}
                    Ok((LineResult::State(new_state), fut)) => {
                        last_image = fut;
                        state = new_state;
                    }
                    Ok((LineResult::Exit, None)) => {
                        break;
                    }
                    Ok((_, _)) => unimplemented!(),
                    Err(()) => {}
                }
            }

            if !state.interactive {
                return Ok(());
            }

            state.lines.push(vec!["RUN".to_owned(), ("pwd").to_owned()]);

            let exec_results = do_line(&docker, &state).unwrap();
            state.lines.pop();
            state.pwd = exec_results.output[0].trim().to_owned();

            loop {
                let prompt = &(state.pwd.clone() + " ");
                print!("{}", prompt);
                std::io::stdout().lock().flush().unwrap();
                let readline = rl.readline(prompt);
                match readline {
                    Ok(line) => {
                        rl.add_history_entry(line.as_ref());

                        let mut next_state = state.clone();
                        next_state.image_name = match last_image {
                            None => next_state.image_name,
                            Some(future) => *await!(future),
                        };
                        last_image = None;

                        let result = parse_line(&line, &next_state, &docker);
                        visitor.command_run(
                            &line,
                            &next_state,
                            match &result {
                                Ok((line_result, _)) => Ok(line_result),
                                Err(()) => Err(()),
                            },
                        );
                        match result {
                            Ok((LineResult::NoOp, None)) => {}
                            Ok((LineResult::State(new_state), fut)) => {
                                last_image = fut;
                                state = new_state;
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
            rl.save_history("Dockerfile.dockershell").unwrap();
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
    NoOp, // E.g. print state...
    State(State),
}

type FutureImage = Pin<Box<Future<Output = Box<String>>>>;

pub fn parse_line(
    mut line: &str,
    state: &State,
    docker: &Docker,
) -> Result<(LineResult, Option<FutureImage>)> {
    assert_eq!(state.lines[0][0], "FROM");
    line = line.trim();
    match line {
        "" => Ok((LineResult::NoOp, None)),
        "exit" => {
            println!("Dockerfile of session:");
            print_dockerfile(&state.lines);
            Ok((LineResult::Exit, None))
        }
        "debug" => {
            let mut state = state.clone();
            state.debug = !state.debug;
            Ok((LineResult::State(state), None))
        }
        // TODO: undo 3 should remove 3rd item.
        // Replay breaks - fix then type continue.
        "undo" => {
            let mut state = state.clone();
            let item = state.lines.pop();
            println!("Undone: {:?}", item);
            Ok((LineResult::State(state), None))
        }
        "layers" => {
            print_layers(&state.lines);
            Ok((LineResult::NoOp, None))
        }
        _ => {
            let mut state = state.clone();
            if line.starts_with("cd ") || line == "cd" {
                state.lines.push(vec![
                    "RUN".to_owned(),
                    (line.to_string() + " ; pwd").to_owned(),
                ]);
                let exec_results = do_line(&docker, &state).unwrap();
                state.lines.pop();

                if state.debug {
                    println!("DIR SET TO {:?}", exec_results.output[0].trim());
                }
                state.pwd = exec_results.output[0].trim().to_owned();
                state
                    .lines
                    .push(vec!["WORKDIR".to_owned(), state.pwd.clone()]);
                Ok((LineResult::State(state), Some(exec_results.image_name)))
            } else {
                state.lines.push(vec!["RUN".to_owned(), line.to_owned()]);
                let exec_result = do_line(&docker, &state);

                match exec_result {
                    Ok(ExecResults {
                        state_change: true,
                        image_name,
                        ..
                    }) => Ok((LineResult::State(state), Some(image_name))),
                    Ok(ExecResults {
                        state_change: false,
                        ..
                    }) => {
                        let removed = state.lines.remove(state.lines.len() - 1);
                        if state.debug {
                            println!("No state change, removed {:?}", removed);
                        }
                        Ok((LineResult::NoOp, None))
                    }
                    Err(()) => Err(()),
                }
            }
        }
    }
}

pub struct ExecResults {
    pub state_change: bool,
    pub output: Vec<String>,
    pub image_name: Pin<Box<Future<Output = Box<String>>>>,
}

/// Ok means the command was executed. Err means that docker couldn't find the command...
fn do_line(docker: &Docker, state: &State) -> Result<ExecResults> {
    let container_name: String = rand::thread_rng().gen_range(0., 1.3e4).to_string();
    assert_eq!(state.lines[0][0], "FROM");

    let mut host_config = ContainerHostConfig::new();
    host_config.auto_remove(false);
    if state.debug {
        println!("img to use: {}", &state.image_name);
    }

    let mut create = ContainerCreateOptions::new(&state.image_name);
    create.tty(state.tty);

    let mut args = state.lines.last().unwrap().clone();
    args.remove(0); //assert [0] == RUN
    if state.debug {
        println!("running cmd: {:?}", &args);
    }

    create.cmd(state.shell.clone());
    create.cmd("-c".to_owned());
    create.cmd(args.join(" "));

    create.host_config(host_config);

    let container = docker
        .create_container(Some(&container_name), &create)
        .unwrap();
    let mut results = Vec::<String>::new();

    if state.tty {
        let res = docker
            .attach_container_tty(&container.id, None, true, true, true, true, true)
            .unwrap();
        let result = docker.start_container(&container.id);

        match result {
            Ok(_) => {
                if state.debug {
                    println!(
                        "starting container id  {} with name  {} ",
                        container.id, &container_name
                    );
                }

                let mut raw_stdout = std::io::stdout().into_raw_mode().unwrap();
                let mut line_reader = BufReader::new(res);

                loop {
                    let mut buf = String::new(); // [0u8;50];
                    let size_result = line_reader.read_line(&mut buf);
                    if let Ok(size) = size_result {
                        raw_stdout.write_all(buf.as_bytes()).unwrap();
                        if size == 0 {
                            //TODO dont' do this.
                            break;
                        }
                        results.push(buf);
                    }
                }
            }
            Err(err) => {
                println!("{:?}", err);
                return Err(());
            }
        }
    } else {
        // non-tty mode kept for tests for now....
        let res = docker
            .attach_container(&container.id, None, true, true, true, true, true)
            .unwrap();
        let result = docker.start_container(&container.id);

        match result {
            Ok(_) => {
                if state.debug {
                    println!(
                        "starting container id  {} with name  {} ",
                        container.id, &container_name
                    );
                }

                let cont: AttachContainer = res.into();
                let mut line_reader = BufReader::new(cont.stdout);

                loop {
                    let mut buf = String::new(); // [0u8;50];
                    let size_result = line_reader.read_line(&mut buf);
                    if let Ok(size) = size_result {
                        print!("{}", buf);
                        if size == 0 {
                            break;
                        }
                        results.push(buf);
                    }
                }
            }
            Err(err) => {
                println!("{:?}", err);
                return Err(());
            }
        }
    }
    println!();

    let search_name = String::from("/") + &container_name;
    let mut filters = ContainerFilters::new();
    filters.name(&search_name);

    let res = docker.list_containers(None, None, None, filters).unwrap();
    let container = res.first().unwrap();

    let changes = docker.filesystem_changes(&container);
    let state_change = match changes {
        Ok(some) => {
            if state.debug {
                println!("CHANGES: {:?}", some);
            };
            true
        }
        Err(_none) => {
            if state.debug { /*println!("CHANGES: {:?}", none);*/ };
            false
        }
    };

    docker
        .remove_container(&container_name, None, Some(true), None)
        .unwrap();

    let image_name = String::from("img_") + &container_name;
    let future_image = build_image(image_name, state.lines.clone(), state.debug).boxed();
    Ok(ExecResults {
        state_change,
        output: results,
        image_name: future_image,
    })
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
    use crate::{do_line, ExecResults, State};
    use dockworker::Docker;
    use futures::executor::block_on;

    #[test]
    fn initial_command() {
        let docker = Docker::connect_with_defaults().unwrap();
        let state = State {
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
            interactive: false,
        };

        let exec_results: ExecResults = do_line(&docker, &state).unwrap();

        //assert!(!exec_results.state_change);//TODO this is not right.
        let _x: Vec<_> = exec_results
            .output
            .iter()
            .map(|line| println!("{}", line))
            .collect();
        assert!(exec_results
            .output
            .iter()
            .any(|s| s.contains("Hello World")));
    }

    // We copy echo as first command so that the second command depends upon the first
    //    /// and would fail against the base alpine:edge image.
    //    #[test]
    //    fn second_command() {
    //        block_on(async {
    //            let docker = Docker::connect_with_defaults().unwrap();
    //            let cmds = vec! [
    //            vec!["FROM".to_owned(), "alpine:edge".to_owned()],
    //            vec!["RUN".to_owned(), "/bin/sh".to_owned(), "-c".to_owned(), "echo 'Hello World' > /tmp/file".to_owned()],
    //            vec!["RUN".to_owned(), "/bin/cat".to_owned(), "/tmp/file".to_owned()],
    //            ];
    //            let mut first = cmds.clone();
    //            first.pop();
    //
    //            let mut state = State{
    //                lines: first,
    //                debug:true,
    //                tty: false,
    //                image_name: "alpine:edge".to_owned(),
    //                pwd: "/bin".to_owned(),
    //                shell: "/bin/sh".to_owned(),
    //                interactive: false,
    //            };
    //
    //            let exec_results1 : ExecResults = do_line(&docker, &state).unwrap();
    //
    //            state.lines = cmds;
    //            state.image_name = *await!(exec_results1.image_name);
    //
    //            let exec_results: ExecResults = do_line(&docker, &state).unwrap();
    //
    //            //assert!(!exec_results.state_change);
    //            let _x : Vec<_> = exec_results.output.iter().map( | line | println ! ("CMD_OUTPUT: {}", line)).collect();
    ////            let _x : Vec<_> = exec_results.output.iter().map( | line | println ! ("CMD_OUTPUT: {}", line)).collect();
    //            assert!(exec_results.output.iter().any( | s |s.contains("Hello World")));
    //        });
    //    }
}
