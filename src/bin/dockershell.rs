use dockershell::*;

fn main() {
    let state = State {
        debug: false,
        tty: true,
        lines: vec![vec!["FROM".to_owned(), "alpine:edge".to_owned()]],
        image_name: "alpine:edge".to_owned(),
        pwd: String::new(),
        shell: "/bin/sh".to_owned(),
    };

    interpreter_loop_from_stdin(state).unwrap();
}
