use rustyline::error::ReadlineError;
use rustyline::Editor;

use dockworker::*;
use dockworker::container::*;
use rand::Rng;
use std::io::Error;
use std::io::{Write,BufReader, BufRead};
use std::path::Path;
use std::fs::File;
use tar::Builder;

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
fn try_do() -> Result<(), Error> {
    //let docker = Docker::connect_with_defaults().unwrap();

    let mut rl = Editor::<()>::new();
    if rl.load_history("Dockerfile.dockershell").is_err() {
        println!("No previous history.");
    }
    let mut lines = Vec::<String>::new();
    lines.push(String::from("FROM alpine:edge"));
    let mut debug = false;
    loop {
        print!(">> ");
        std::io::stdout().lock().flush().unwrap();
        let readline = rl.readline(">> ");
        match readline {
            Ok(mut line) => {
                line = line.trim().to_string();
                match line.as_ref() {
                    "" => {},
                    "debug" => { debug= !debug; },
                    "undo" => { let item = lines.pop(); println!("Undone: {:?}", item); }
                    "layers" => {
                        for (i, l) in lines.iter().enumerate() {
                            println!("{}: {}", i, l);
                        }
                    },
                    _ => {
                    rl.add_history_entry(line.as_ref());
                    //println!("Line: {}", &line);

                    if line.starts_with("cd ") {
                        lines.push(String::from("WORKDIR ") + &line["cd ".len()..]); // /bin/sh -c
                        lines.push(String::from("RUN pwd")); // /bin/sh -c
                        do_line(&lines, debug).unwrap();
                    } else {
                        lines.push(String::from("RUN ") + &line); // /bin/sh -c
                        let exec_result = do_line(&lines, debug);
                        match exec_result {
                            Ok(true) => {
                                lines.remove(lines.len() - 1); },//stateless
                            Ok(false) => {},
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
}

/// Ok means the command was executed. Err means that docker couldn't find the command...
fn do_line(command_lines: &Vec<String>, debug: bool) -> Result<bool, ()>{
    {
        let mut dockerfile = File::create("Dockerfile").unwrap();
        dockerfile.write_all(command_lines[..command_lines.len() - 1].join("\n").as_bytes()).unwrap();
    }
    // Create tar file
    {
        let tar_file = File::create("image.tar").unwrap();
        let mut a = Builder::new(tar_file);
        a.append_path("Dockerfile").unwrap();
    }
    let container_name: String = rand::thread_rng().gen_range(0., 1.3e4).to_string();
    let image_name = String::from("img_") + &container_name;

    let options = ContainerBuildOptions{
        t: vec![image_name.to_owned()],
        ..ContainerBuildOptions::default()
    };

    let docker = Docker::connect_with_defaults().unwrap();
    let res = docker.build_image(options, Path::new("image.tar")).unwrap();

    //Was it a stateless operation?
    let pattern = r#"{"stream":"Removing intermediate container"#;
    let mut return_value = Ok(false);
    for line in BufReader::new(res).lines() {
        let buf = line.unwrap();
        if debug {
            println!("{}", &buf);
        }
        if buf.starts_with(pattern) {
           // println!("Found one!");
            return_value = Ok(true) // stateless transformation like 'ls' (n-1)
        }
    }

    let mut host_config = ContainerHostConfig::new();
    host_config.auto_remove(true);
    let img_to_use = &(String::from(image_name) + ":latest");
    if debug {
        println!("img to use: {}", img_to_use);
    }
    let mut create = ContainerCreateOptions::new(img_to_use);
    let mut args : Vec<String> = Vec::new();

    let command_line = &command_lines[command_lines.len() - 1];
    for arg in command_line.split(' ') {
        args.push(String::from(arg));
    }
    args.remove(0); //asserrt [0] == RUN
    if debug {
        println!("running cmd: {:?}", &args);
    }
    for ar in args {
        create.cmd(ar);
    }

    create.host_config(host_config);

    let container = docker.create_container(Some(&container_name), &create).unwrap();
    let res = docker
        .attach_container(&container.id, None, true, true, false, true, false)
        .unwrap();
    let result = docker.start_container(&container.id);

    let mut results = Vec::<String>::new();
    match result {
        Ok(_) => {
            if debug {
                println!("starting container id  {} with name  {} ", container.id, &container_name);
            }
            let cont: AttachContainer = res.into();
            let mut line_reader = BufReader::new(cont.stdout);

            loop {
                let mut line = String::new();
                let size = line_reader.read_line(&mut line).unwrap();
                print!("{}", line);
                if size == 0 {
                    break;
                }
                results.push(line);
            }
        },
        Err(err) => {println!("{:?}", err); return Err(())}
    }
    println!();
    return_value
}


mod tests {
//    use super::*;

    #[test]
    fn if_output_always_same_return_earliest_command() {

    }
}
