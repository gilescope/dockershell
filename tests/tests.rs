mod tests {
    use dockershell::{interpreter_loop_from_file, ExecListener, LineResult, State};

    struct Checker<'l> {
        next: usize,
        expected_results: Vec<Result<&'l LineResult, ()>>,
    }

    impl<'l> ExecListener for Checker<'l> {
        fn command_run(
            &mut self,
            _line: &str,
            _state: &State,
            line_result: Result<&LineResult, ()>,
        ) {
            println!("checking step index {}", self.next);
            let mut expected = self.expected_results[self.next].clone();
            let line_result_ref: LineResult;

            // Ignore image_name in comparison as it is random generated.
            if let Ok(LineResult::State(result_state, _output)) = line_result {
                if let Ok(LineResult::State(expected_state, expected_output)) = expected {
                    let mut expected_st = (*expected_state).clone();
                    expected_st.image_name = result_state.image_name.clone();
                    line_result_ref = LineResult::State(expected_st, expected_output.to_owned());
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
                vec!["cd ..".to_owned()],
            ],
            ..State::test()
        };

        interpreter_loop_from_file(
            state.clone(),
            &mut Checker {
                next: 0,
                expected_results: vec![Ok(&LineResult::State(
                    State {
                        lines: vec![
                            vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                            vec!["WORKDIR".to_owned(), "/".to_owned()],
                        ],
                        pwd: "/".to_owned(),
                        ..state
                    },
                    "/\n".to_owned(),
                ))],
            },
        )
        .unwrap();
    }

    #[test]
    fn state_change_cd_root() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["cd /".to_owned()],
                vec!["mkdir temp".to_owned()],
            ],
            ..State::test()
        };

        interpreter_loop_from_file(
            state.clone(),
            &mut Checker {
                next: 0,
                expected_results: vec![
                    Ok(&LineResult::State(
                        State {
                            lines: vec![
                                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                                vec!["WORKDIR".to_owned(), "/".to_owned()],
                            ],
                            pwd: "/".to_owned(),
                            ..state.clone()
                        },
                        "/\n".to_owned(),
                    )),
                    Ok(&LineResult::State(
                        State {
                            lines: vec![
                                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                                vec!["WORKDIR".to_owned(), "/".to_owned()],
                                vec!["RUN".to_owned(), "mkdir temp".to_owned()],
                            ],
                            pwd: "/".to_owned(),
                            ..state
                        },
                        "".to_owned(),
                    )),
                ],
            },
        )
        .unwrap();
    }

    #[test]
    fn state_change_mk_dir() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["mkdir /bin/foo".to_owned()],
            ],
            ..State::test()
        };

        interpreter_loop_from_file(
            state.clone(),
            &mut Checker {
                next: 0,
                expected_results: vec![Ok(&LineResult::State(
                    State {
                        lines: vec![
                            vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                            vec!["RUN".to_owned(), "mkdir /bin/foo".to_owned()],
                        ],
                        ..state
                    },
                    "".to_owned(),
                ))],
            },
        )
        .unwrap();
    }

    #[test]
    fn mk_and_rm_dir() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["mkdir /bin/foo".to_owned()],
                vec!["rmdir /bin/foo".to_owned()],
            ],
            ..State::test()
        };

        interpreter_loop_from_file(
            state.clone(),
            &mut Checker {
                next: 0,
                expected_results: vec![
                    Ok(&LineResult::State(
                        State {
                            lines: vec![
                                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                                vec!["RUN".to_owned(), "mkdir /bin/foo".to_owned()],
                            ],
                            ..state.clone()
                        },
                        "".to_owned(),
                    )),
                    Ok(&LineResult::State(
                        State {
                            lines: vec![
                                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                                vec!["RUN".to_owned(), "mkdir /bin/foo".to_owned()],
                                vec!["RUN".to_owned(), "rmdir /bin/foo".to_owned()],
                            ],
                            ..state
                        },
                        "".to_owned(),
                    )),
                ],
            },
        )
        .unwrap();
    }

    #[test]
    fn no_op_with_output_to_std_err() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec![">&2 echo Hello".to_owned()],
            ],
            ..State::test()
        };

        interpreter_loop_from_file(
            state.clone(),
            &mut Checker {
                next: 0,
                expected_results: vec![Ok(&LineResult::NoOp("Hello\n".to_owned()))],
            },
        )
        .unwrap();
    }

    #[test]
    fn change_to_bad_dir_should_not_panic() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["cd ....".to_owned()],
                vec!["echo Hi".to_owned()],
            ],
            ..State::test()
        };

        interpreter_loop_from_file(
            state.clone(),
            &mut Checker {
                next: 0,
                expected_results: vec![Err(()), Ok(&LineResult::NoOp("Hi\n".to_owned()))],
            },
        )
        .unwrap();
    }

    #[test]
    fn change_dir_should_go_up_a_dir() {
        let state = State {
            lines: vec![
                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                vec!["cd ..".to_owned()],
                vec!["pwd".to_owned()],
                vec!["pwd".to_owned()],
            ],
            ..State::test()
        };

        interpreter_loop_from_file(
            state.clone(),
            &mut Checker {
                next: 0,
                expected_results: vec![
                    Ok(&LineResult::State(
                        State {
                            lines: vec![
                                vec!["FROM".to_owned(), "alpine:edge".to_owned()],
                                vec!["WORKDIR".to_owned(), "/".to_owned()],
                            ],
                            pwd: "/".to_owned(),
                            ..state.clone()
                        },
                        "/\n".to_owned(),
                    )),
                    Ok(&LineResult::NoOp("/\n".to_owned())),
                    Ok(&LineResult::NoOp("/\n".to_owned())),
                ],
            },
        )
        .unwrap();
    }

}
