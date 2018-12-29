mod tests {
    use dockershell::{State, ExecListener, LineResult, interpreter_loop};

    struct Checker<'l> {
        next: usize,
        expected_results: Vec<Result<&'l LineResult, ()>>
    }

    impl <'l> ExecListener for Checker<'l> {
        fn command_run(&mut self, _line: &str, _state: &State, line_result: Result<&LineResult, ()>) {
            println!("checking step index {}", self.next);
            let mut expected = self.expected_results[self.next].clone();
            let line_result_ref : LineResult;

            // Ignore image_name in comparison as it is random generated.
            if let Ok(LineResult::State(result_state)) = line_result
            {
                if let Ok(LineResult::State(expected_state)) = expected {
                    let mut expected_st = (*expected_state).clone();
                    expected_st.image_name = result_state.image_name.clone();
                    line_result_ref = LineResult::State(expected_st);
                    expected = Ok(&line_result_ref);
                }
            }
            assert_eq!(expected, line_result);
            self.next += 1;
        }
    }

    #[test]
    fn state_change_cd_up() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["cd ..".to_owned()]
            ],
            .. State::test()
        };

        interpreter_loop(state.clone(), &mut Checker{
            next: 0,
            expected_results: vec![Ok(&LineResult::State(
                State {
                    lines: vec![
                        vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                        vec!["WORKDIR".to_owned(), "/".to_owned()]],
                    pwd: "/".to_owned(),
                    .. state
                }
            ))]
        }).unwrap();
    }

    #[test]
    fn state_change_mk_dir() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["mkdir /bin/foo".to_owned()]
            ],
            .. State::test()
        };

        interpreter_loop(state.clone(), &mut Checker{
            next: 0,
            expected_results: vec![Ok(&LineResult::State(
                State {
                    lines: vec![
                        vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                        vec!["RUN".to_owned(), "mkdir /bin/foo".to_owned()]],
                    .. state
                }
            ))]
        }).unwrap();
    }

    #[test]
    fn mk_and_rm_dir() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["mkdir /bin/foo".to_owned()],
                vec!["rmdir /bin/foo".to_owned()]
            ],
            .. State::test()
        };

        interpreter_loop(state.clone(), &mut Checker {
            next: 0,
            expected_results: vec![
                Ok(&LineResult::State(
                    State {
                        lines: vec![
                            vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                            vec!["RUN".to_owned(), "mkdir /bin/foo".to_owned()]],
                        .. state.clone()
                    }
                )),
               Ok(&LineResult::State(
                   State {
                       lines: vec![
                           vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                           vec!["RUN".to_owned(), "mkdir /bin/foo".to_owned()],
                           vec!["RUN".to_owned(), "rmdir /bin/foo".to_owned()]
                       ],
                       .. state
                   }
               ))
            ]
        }).unwrap();
    }

    #[test]
    fn no_op_grep() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["grep".to_owned()]
            ],
            .. State::test()
        };

        interpreter_loop(state.clone(), &mut Checker{
            next: 0,
            expected_results: vec![Ok(&LineResult::NoOp)]
        }).unwrap();
    }
}