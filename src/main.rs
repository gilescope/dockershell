#![deny(trivial_casts, trivial_numeric_casts, unused_import_braces, unused_qualifications)]
#![allow(unused_imports)]
#![feature(pin)]
#![feature(futures_api)]
#![feature(await_macro)]
#![feature(async_await)]

use std::pin::Pin;
use std::io::{Write,BufReader, BufRead};
use std::path::Path;
use std::fs::File;

use futures::future::FutureExt;
use futures::executor::block_on;
use futures::Future;

use termion::raw::IntoRawMode;

use rustyline::error::ReadlineError;
use rustyline::Editor;

use dockworker::*;
use dockworker::container::*;

use rand::Rng;

use tar::Builder;

type Result<T> = std::result::Result<T,()>;

fn main() {
     try_do().unwrap();
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
fn try_do() -> Result<()> {
    let docker = Docker::connect_with_defaults().unwrap();

    block_on(async {
        let mut rl = Editor::<()>::new();
        if rl.load_history("Dockerfile.dockershell").is_err() {
            println!("No previous history.");
        }
        let mut lines = Vec::<Vec<String>>::new();
        lines.push(vec!["FROM".to_owned(), "alpine:edge".to_owned()]);
        let mut debug = false;
        let mut tty = true;
        let mut last_image : Option<Pin<Box<Future<Output=Box<String>>>>> = None;
        let shell = "/bin/sh";

        lines.push(vec![
            "RUN".to_owned(),
            shell.to_owned(),
            "-c".to_owned(),
            ("pwd").to_owned()
        ]);
        let exec_results = do_line(&docker, &lines, debug, tty, "alpine:edge".to_owned()).unwrap();
        lines.pop();
        let mut pwd = exec_results.output[0].trim().to_owned();

        loop {
            let prompt = &(pwd.clone() + " ");
            print!("{}", prompt);
            std::io::stdout().lock().flush().unwrap();
            let readline = rl.readline(prompt);
            match readline {
                Ok(mut line) => {
                    line = line.trim().to_string();
                    match line.as_ref() {
                        "" => {},
                        "exit" => {
                            println!("Dockerfile of session:");
                            for l in lines.iter() {
                                println!("{}", l.join(" "));
                            }
                            return Ok(())
                        }
                        "debug" => { debug= !debug; },
                        "tty" => { tty = !tty },
                        // TODO: undo 3 should remove 3rd item.
                        // Replay breaks - fix then type continue.
                        "undo" => { let item = lines.pop(); println!("Undone: {:?}", item); }
                        "layers" => {
                            for (i, l) in lines.iter().enumerate() {
                                println!("{}: {:?}", i, l);
                            }
                        },
                        _ => {
                        rl.add_history_entry(line.as_ref());
                        //println!("Line: {}", &line);

                        let image_to_use_opt = last_image;
                        last_image = None;
                        let image_to_use : String = match image_to_use_opt {
                            None => "alpine:edge".to_owned(),
                            Some(future) => (*await!(future)).clone()
                        };

                        if line.starts_with("cd ") || line == "cd" {
                            lines.push(vec![
                                "RUN".to_owned(),
                                shell.to_owned(),
                                "-c".to_owned(),
                                (line.clone() + " ; pwd").to_owned()
                            ]);
                            let exec_results = do_line(&docker, &lines, debug, tty, image_to_use).unwrap();
                            lines.pop();

                            if debug { println!("DIR SET TO {:?}", exec_results.output[0].trim()); }
                            pwd = exec_results.output[0].trim().to_owned();
                            lines.push(vec!["WORKDIR".to_owned(),pwd.clone()]);
                        } else {
                            lines.push(vec![
                                "RUN".to_owned(),
                                shell.to_owned(),
                                "-c".to_owned(),
                                line.clone()
                            ]);
                            //lines.push(vec![String::from("RUN"), line.clone()]); // /bin/sh -c
                            let exec_result = do_line(&docker, &lines, debug, tty, image_to_use);
                            match exec_result {
                                Ok(ExecResults{state_change:true, output: _, image_name }) => {
                                    last_image = Some(image_name);
                                //    lines.remove(lines.len() - 1);
                                },//stateless
                                Ok(ExecResults{state_change:false, output: _, image_name: _ }) => {},
                                Err(()) => { lines.pop(); }
                            }
                        }
                    }
                }
                },
                Err(ReadlineError::Interrupted) => {
                    println!("CTRL-C");
                    break
                },
                Err(ReadlineError::Eof) => {
                    println!("CTRL-D");
                    break
                },
                Err(err) => {
                    println!("Error: {:?}", err);
                    break
                }
            }
        }
        rl.save_history("Dockerfile.dockershell").unwrap();
    Ok(())
    })
}

pub struct ExecResults {
    pub state_change: bool,
    pub output: Vec<String>,
    pub image_name: Pin<Box<Future<Output=Box<String>>>>
}

/// Ok means the command was executed. Err means that docker couldn't find the command...
fn do_line(docker: &Docker, command_lines: &Vec<Vec<String>>, debug: bool, tty: bool, image_name: String) -> Result<ExecResults>{
    let container_name: String = rand::thread_rng().gen_range(0., 1.3e4).to_string();

    let mut host_config = ContainerHostConfig::new();
    host_config.auto_remove(true);
    let mut img_to_use = String::new();
    img_to_use.clone_from(&image_name);
    if debug {
        println!("img to use: {}", img_to_use);
    }

    let mut create = ContainerCreateOptions::new(&img_to_use);
    create.tty(tty);

    let mut args = command_lines.last().unwrap().clone();
    args.remove(0); //assert [0] == RUN
    if debug {
        println!("running cmd: {:?}", &args);
    }
    for arg in args {
        create.cmd(arg.to_string());
    }
    create.host_config(host_config);

    let container = docker.create_container(Some(&container_name), &create).unwrap();
    let mut results = Vec::<String>::new();

    if tty {
            let res = docker
                .attach_container_tty(&container.id, None, true, true, true, true, true)
                .unwrap();
            let result = docker.start_container(&container.id);

            match result {
                Ok(_) => {
                    if debug {
                        println!("starting container id  {} with name  {} ", container.id, &container_name);
                    }

                    let mut raw_stdout = std::io::stdout().into_raw_mode().unwrap();
                    let mut line_reader = BufReader::new(res);

                    loop {
                        let mut buf = String::new();// [0u8;50];
                        let size_result = line_reader.read_line(&mut buf);
                        if let Ok(size) = size_result {
                            raw_stdout.write_all(buf.as_bytes()).unwrap();
                            if size == 0 {//TODO dont' do this.
                                break;
                            }
                            results.push(buf);
                        }
                    }
                },
                Err(err) => {
                    println!("{:?}", err);
                    return Err(())
                }
            }
        }
    else {
        // non-tty mode kept for tests for now....
        let res = docker
            .attach_container(&container.id, None, true, true, true, true, true)
            .unwrap();
        let result = docker.start_container(&container.id);

        match result {
            Ok(_) => {
                if debug {
                    println!("starting container id  {} with name  {} ", container.id, &container_name);
                }

            let cont: AttachContainer = res.into();
                let mut line_reader = BufReader::new(cont.stdout);

                loop {
                    let mut buf = String::new();// [0u8;50];
                    let size_result = line_reader.read_line(&mut buf);
                    if let Ok(size) = size_result {
                        print!("{}", buf);
                        if size == 0 {
                            break;
                        }
                        results.push(buf);
                    }
                }
            },
            Err(err) => {
                println!("{:?}", err);
                return Err(())
            }
        }
    }
    println!();

//    docker.

    let commands = command_lines.clone();
    let image_name :String = String::from("img_") + &container_name;

    let future_image = build_image(image_name, commands, debug).boxed();
    Ok(ExecResults{ state_change:true, output:results, image_name: future_image})
}

async fn build_image(image_name: String, command_lines: Vec<Vec<String>>, debug: bool) -> Box<String> {
    if debug { println!("building img {} as {:?}", &image_name, &command_lines) }
    let docker = Docker::connect_with_defaults().unwrap();
    {
        let mut dockerfile = File::create("Dockerfile").unwrap();
        let lines : Vec<String> = command_lines.iter().map(|args| args.join(" ")).collect();
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

    //Was it a stateless operation?
    //let pattern = r#"{"stream":"Removing intermediate container"#;
    //let mut return_value = false;
    for line in BufReader::new(res).lines() {
        let buf = line.unwrap();
        if debug {
            println!("{}", &buf);
        }
//        if buf.starts_with(pattern) {
//            // println!("Found one!");
//            return_value = true; // stateless transformation like 'ls' (n-1)
//        }
    }
    if debug { println!("built image {}", &image_name); }
    Box::new(image_name.to_owned())
}

mod tests {
    use crate::{ExecResults, do_line};
    use futures::executor::block_on;
    use dockworker::Docker;

    #[test]
    fn initial_command() {
        let docker = Docker::connect_with_defaults().unwrap();
        let exec_results : ExecResults = do_line(&docker, &vec![
            vec!["FROM".to_owned(), "alpine:edge".to_owned()],
            vec!["RUN".to_owned(), "/bin/echo".to_owned(), "Hello World".to_owned()],
        ], true, false, "alpine:edge".to_owned()).unwrap();

        //assert!(!exec_results.state_change);//TODO this is not right.
        let _x : Vec<_> = exec_results.output.iter().map(|line| println!("{}", line)).collect();
        assert!(exec_results.output.iter().any(|s|s.contains("Hello World")));
    }

    /// We copy echo as first command so that the second command depends upon the first
    /// and would fail against the base alpine:edge image.
    #[test]
    fn second_command() {
        block_on(async {
            let docker = Docker::connect_with_defaults().unwrap();
            let cmds = vec! [
            vec!["FROM".to_owned(), "alpine:edge".to_owned()],
            vec!["RUN".to_owned(), "/bin/sh".to_owned(), "-c".to_owned(), "echo 'Hello World' > /tmp/file".to_owned()],
            vec!["RUN".to_owned(), "/bin/cat".to_owned(), "/tmp/file".to_owned()],
            ];
            let mut first = cmds.clone();
            first.pop();

            let exec_results1 : ExecResults = do_line(&docker, &first, true, false, "alpine:edge".to_owned()).unwrap();

            let img = await!(exec_results1.image_name);

            let exec_results: ExecResults = do_line(&docker, &cmds, true, false, (*img).clone()).unwrap();

            //assert!(!exec_results.state_change);
            let _x : Vec<_> = exec_results.output.iter().map( | line | println ! ("CMD_OUTPUT: {}", line)).collect();
//            let _x : Vec<_> = exec_results.output.iter().map( | line | println ! ("CMD_OUTPUT: {}", line)).collect();
            assert!(exec_results.output.iter().any( | s |s.contains("Hello World")));
        });
    }
}
