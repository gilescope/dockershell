use std::io::{BufRead, BufReader, Read, Write};

use dockworker::container::*;
use dockworker::*;
use rand::Rng;
use termion::raw::IntoRawMode;

use super::State;

type Result<T> = std::result::Result<T, ()>;

pub struct ExecResults {
    pub state_change: bool,
    pub output: String,
    pub container_name: String,
}

/// Executes the last command of the state.
/// Ok means the command was executed. Err means that docker couldn't find the command...
pub(crate) fn execute_command(docker: &Docker, state: &State) -> Result<ExecResults> {
    if state.debug {
        println!("do_line: {:?}", &state);
    }

    let container_name: String = rand::thread_rng().gen_range(0., 1.3e4).to_string();
    assert_eq!(state.lines[0][0], "FROM");

    let mut host_config = ContainerHostConfig::new();
    host_config.auto_remove(false);

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

                let mut cont: AttachContainer = res.into();
                let mut buf = vec![];
                // These streams are split out in non-tty mode:
                cont.stdout.read_to_end(&mut buf).unwrap();
                cont.stderr.read_to_end(&mut buf).unwrap();
                results.push(String::from_utf8(buf).unwrap());
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
    let container: &Container = res.first().unwrap();

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

    Ok(ExecResults {
        state_change,
        output: results.join("\n"),
        container_name,
    })
}
