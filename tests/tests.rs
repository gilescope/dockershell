//extern crate dockershell;

mod tests {
    use dockershell::{ExecResults, State, parse_line};
//    use futures::executor::block_on;
    use dockworker::Docker;
    //use dockershell::*;

    enum Cmd {
        NoOp(&'static str),
        ChDir(&'static str, &'static str),
        State(&'static str)
    }

    #[test]
    fn run_transcripts() {
        let docker = Docker::connect_with_defaults().unwrap();

        let trans = vec![
            Cmd::NoOp("cd ."),
            Cmd::ChDir("cd ..", "/"),
            Cmd::NoOp("pwd"),
            Cmd::State("mkdir /giles")
        ];

        for line in trans {
            let mut state = State{
                lines: vec![],
                debug:true,
                tty: false,
                image_name: "alpine:edge".to_owned(),
                pwd: "/bin".to_owned(),
                shell: "/bin/sh".to_owned()
            };
            match line {
                Cmd::NoOp(cmd) => {
                    parse_line(cmd.to_string(), &state, &docker);
                },
                _ => {}

            }
        }


//        let docker = Docker::connect_with_defaults().unwrap();
//        let exec_results: ExecResults = do_line(&docker, &vec![
//            vec!["FROM".to_owned(), "alpine:edge".to_owned()],
//            vec!["RUN".to_owned(), "/bin/echo".to_owned(), "Hello World".to_owned()],
//        ], true, false, "alpine:edge".to_owned()).unwrap();
//
//        //assert!(!exec_results.state_change);//TODO this is not right.
//        let _x: Vec<_> = exec_results.output.iter().map(|line| println!("{}", line)).collect();
//        assert!(exec_results.output.iter().any(|s| s.contains("Hello World")));
    }
}