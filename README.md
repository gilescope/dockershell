# DockerShell

## What does it do?

When you execute a command it will do two things:

   * Show you the results of that command.
   
   * If the command is state-changing it will be recorded in the history.
   
So for example: `cd ..` would change the state and be recorded, but `grep --help` would not.
As you execute commands, the dockershell is building up layers of a docker image.

Built-in shell commands:

   * `layers` prints out the current history of commands.
   * `undo` the last state changing command.
   * `exit` to quit.

On exiting it will print out in Dockerfile format the history.

## Why?

I couldn't find anyone that had tried doing something like this. I was curious how it would 'feel' and if this might be a 
nice way of building up Dockerfiles (step by step) rather than commiting a working container (which would not be rebuildable).

## Command line:

`cargo run` will run a new shell starting from `alpine:edge`.

## Status:

Alpha - colors work in `ls` but full tty commands (like `vi`) don't currently work, but how should editing a file be 
handled anyway - where would one put the state?

TODO: Add command line arguments to choose base image, or to start from a docker file.
TODO: Improve test coverage / error handling. Currently isn't that hard to break.
