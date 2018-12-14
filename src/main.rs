use rustyline::error::ReadlineError;
use rustyline::Editor;

use std::io::{BufReader, BufRead};
use std::io::Error;
use dockworker::*;
use dockworker::container::*;
use rand::Rng;

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
    //do_line(String::from("/bin/echo Hello"));
    let docker = Docker::connect_with_defaults().unwrap();
    println!("{:#?}", docker.system_info().unwrap());

    let mut rl = Editor::<()>::new();
    if rl.load_history(".dockershell.history.txt").is_err() {
        println!("No previous history.");
    }
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_ref());
                println!("Line: {}", line);
                do_line(line);
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
    rl.save_history(".dockershell.history.txt").unwrap();




    //List containers
//    let filter = ContainerFilters::new();
//    let containers = docker.list_containers(None, None, None, filter).unwrap();
//
//    containers.iter().for_each(|c| {
//        docker.stop_container(&c.Id, Duration::from_secs(2)).unwrap();
//        println!("ACTION: stopped {:?}", c);
//        docker.remove_container(&c.Id, None, None, None).unwrap();
//        println!("ACTION: removed {:?}", c);
//    });

//    //Create a continaer against the image
//    let mut create = ContainerCreateOptions::new(&image);
//    create.tty(true);
//    create.entrypoint(vec!["/bin/sh".to_string(), "echo Hello world".to_string()]);
//
//    //instantiate container
//    let container: CreateContainerResponse = docker.create_container(
//        Some("my_container_name"), &create).unwrap();


//    let filter = ContainerFilters::new();
//    let containers = docker.list_containers(None, None, None, filter).unwrap();
//    containers.iter().for_each(|c| {
//        println!("QUERY: status {:?}", c.Status);
//
//        let opts = ContainerListOptions::default();
//        let x = docker.filesystem_changes(c);
//        let info = docker.container_info(c).unwrap();
//        for mount in info.Mounts {
//            println!("mount {:?}", mount);
//        }
//        if let Ok(y) = x {
//            for change in y {
//                println!("FOUND file sys changes::: {:#?}", change);
//            }
//        } else {
//            println!("not found any {}", c)
//        }

//    let mut current_output: Option<String> = None;
//    let mut current_command: Option<String> = None;
//
//    let histories = docker.history_image("myimage:latest");
//    for history in histories {
//        for event in history {
//            //Gradually going back in time....
//            println!("happened {:?} tags: {:?}", event.id, event.tags);
//            // println!("happened {}", event.created_by);
//
//            if let Some(image) = event.id {
//                //Remove any existing container with same name...
//                let container_name = String::from("AA") + &(std::time::SystemTime::now().elapsed().unwrap()).as_secs().to_string();
//                //println!("CONT name : {}", &container_name);
//                docker.remove_container(&container_name, None, Some(true), None);
//
//                //Create container
//                let mut create = ContainerCreateOptions::new(&image);
//                let mut host_config = ContainerHostConfig::new();
//                host_config.auto_remove(false);
//                create.host_config(host_config);
//                let it = command_line.iter();
//                for command in it {
//                    create.cmd(command.clone());
//                }
//
//                let container: CreateContainerResponse = docker.create_container(
//                    Some(&container_name), &create).unwrap();
//
//                docker.start_container(&container.id).unwrap();
//
//                let log_options = ContainerLogOptions {
//                    stdout: true,
//                    stderr: true,
//                    since: None,
//                    timestamps: None,
//                    tail: None,
//                };
//
//                std::thread::sleep(Duration::from_secs(2));
//
//                let mut container_output = String::new();
//
//                let result = docker.log_container_and_follow(&container_name, &log_options);
//                if let Ok(result) = result {
//                    let mut size = 1;
//                    let mut line_reader = BufReader::new(result);
//
//                    while size != 0 {
//                        size = line_reader.read_line(&mut container_output).unwrap();
//                    }
//                }
//                //println!("{:?}", &container_output);
//
//                {
//                    let expected = current_output.get_or_insert(container_output.clone());
//
//                    if expected != &container_output {
//                        println!("{:?} changed to: {:?}", &container_output, &expected);
//                        //Interesting...
//                        println!("next command: {:?}", &current_command);
//                        println!("previous command: {}", &event.created_by);
//                    }
//                }
//                use std::mem; //when match it's a no-op
//                mem::replace(&mut current_output, Some(container_output));
//
//                mem::replace(&mut current_command, Some(event.created_by));
//
//                docker.stop_container(&container.id, Duration::from_secs(2));
//            }
//        }
//    }
//
//
//    let f = File::open("/Users/gilescope/private/strat2/strategy-runner-py/Dockerfile")?;
//    let mut reader = BufReader::new(f);
//
//    let mut lines = String::new();
//
//    reader.read_to_string(&mut lines)?;
//
//    let mut docker_file_so_far = String::new();
//
//    for line in lines.lines() {
//        docker_file_so_far.push('\n');
//        docker_file_so_far.push_str(line);
////        println!("{}", docker_file_so_far);
////        println!("-----------------------");
//    }
    Ok(())
}

fn do_line(command_line: String) {
    let docker = Docker::connect_with_defaults().unwrap();
    let mut host_config = ContainerHostConfig::new();
    host_config.auto_remove(true);
    let mut create = ContainerCreateOptions::new("alpine:edge");
    let mut args : Vec<String> = Vec::new();

    args.push("/bin/sh".into());
    args.push("-c".into());

    for arg in command_line.split(' ') {
        args.push(String::from(arg));
    }
    create.entrypoint(args);

//    create.entrypoint(vec!["/bin/echo".into(),"hi".into()]);
    create.host_config(host_config);
    let container_name: String = rand::thread_rng().gen_range(0., 1.3e4).to_string();

    let container = docker.create_container(Some(&container_name), &create).unwrap();
    docker.start_container(&container.id).unwrap();
    let res = docker
        .attach_container(&container.id, None, true, true, false, true, false)
        .unwrap();
    let cont: AttachContainer = res.into();
    let mut line_reader = BufReader::new(cont.stdout);

    loop {
        let mut line = String::new();
        let size = line_reader.read_line(&mut line).unwrap();
        print!("{:4}: {}", size, line);
        if size == 0 {
            break;
        }
    }
    println!("");

//    docker.

//
//    let docker = Docker::connect_with_defaults().unwrap();
//    let image = "alpine:edge";// "hello-world:linux";
//    let mut create = ContainerCreateOptions::new(&image);
//    create.tty(true);
//    let mut args : Vec<String> = Vec::new();
//
//    for arg in command_line.split(' ') {
//        args.push(String::from(arg));
//    }
//    create.entrypoint(args);
//    let container_name: String = rand::thread_rng().gen_range(0., 1.3e4).to_string();
//
//    //instantiate container
//    let container: CreateContainerResponse = docker.create_container(
//        Some(&container_name), &create).unwrap();
//
//    let res = docker
//        .attach_container(&container.id, None, true, true, true, true, true)
//        .unwrap();
//    let cont: AttachContainer = res.into();
//    let mut line_reader = BufReader::new(cont.stdout);
//    let mut line_reader_err = BufReader::new(cont.stderr);
//
//    println!("Container ID: {}", &container.id);
//    docker.start_container(&container.id).unwrap();
//
//
//    loop {
//        let mut line = String::new();
//        let res = line_reader.read_line(&mut line);
//        match res {
//            Ok(size) => {
//                if size > 0 {
//                    print!("OUT: {:4}: {}", size, line);
//                }
////                if size == 0 {
////                    break;
////                }
//            },
//            Err(e) => {
//                println!("OUT: {}", e);
//            }
//        }
//
//        let res = line_reader_err.read_line(&mut line);
//        match res {
//            Ok(size) => {
//                if size > 0 {
//                    print!("ERR: {:4}: {}", size, line);
//                }
////                if size == 0 {
////                    break;
////                }
//            },
//            Err(e) => {
//                println!("ERR: {}", e);
//            }
//        }
//    }
}


mod tests {
//    use super::*;

    #[test]
    fn if_output_always_same_return_earliest_command() {

    }
}
