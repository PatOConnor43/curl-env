# curl-env

curl-env is a tool aiming to providing shell autocompletion for curl commands
based on OpenAPI specifications.

## Motivation

I _really_ like `curl`. I use it all the time in order to interact with APIs. I
couldn't find any tools that would allow me to generate completion scripts
based on OpenAPI specifications though so here is my attempt at that. With this
tool, I'm hoping I can repeatedly slap the <Tab> key and have my shell
autocomplete the curl command I'm trying to write.

## Installation

Check the releases page for prebuilt binaries.

## Usage

### Activating a subshell with curl-env:

```zsh
curl-env activate --spec ./petstore.yaml --base-url http://localhost:8080
```
This will start a subshell with `curl` autocompletion enabled. You can then
type `curl` followed by <Tab> to see the available endpoints.

To deactivate the subshell, simply type `exit` or press `Ctrl+D`.

### Generating a completion script:

```zsh
curl-env completion --spec ./petstore.yaml --base-url http://localhost:8080
```

This will output a completion script to stdout. You can redirect this to a file
and source it or source it directly in your shell.
For example, to source it directly in your current shell session:

```zsh
source <(curl-env completion --spec ./petstore.yaml --base-url http://localhost:8080)
```

### Completion Examples

```console
$ curl <TAB>
# Shows available URLs for the given OpenAPI spec:
http://localhost:8080/pets
http://localhost:8080/pets/petId

$ curl http://localhost:8080/pets/petId -<TAB>
# Shows available options
--data            -d  -- Pass request body
--data-urlencode      -- Specify query parameter
--get             -G  -- Append request body as query parameters
--help            -h  -- Display help
--output          -o  -- Write output to file
--request         -X  -- Specify request method
--verbose         -v  -- Verbose mode

$ curl http://localhost:8080/pets/petId -G --data-urlencode <TAB>
# Shows available query parameters:
limit=    tags=     status=

$ curl http://localhost:8080/pets/{petId} -X POST -d <TAB>
# Shows json request bodies taken from the `example` or `examples` field in the OpenAPI spec:
{"id":1,"name":"doggie","tag":"dog"}
{"id":2,"name":"cat","tag":"cat"}
```

## Shell Support

This tool currently supports `zsh` only. If you are using `bash` or another
shell, you will not be able to use the completion script feature, but you can
still use the subshell feature as long as you have `zsh` installed.

## Contributing

### Testing
This project uses [cargo-insta](https://crates.io/crates/cargo-insta) to create
snapshots of the output to test against. Insta provides a tool that makes
running these tests and reviewing their output easier. To install it run `cargo
install cargo-insta`. Once this is installed, changes can be reviewed with
`cargo insta test --review`.

If you're just trying to run the tests you can run `cargo test`.

### Cargo dist
This project uses `cargo dist` to build a distributable binary. Specifically this project uses the fork maintained by [astral-sh](https://github.com/astral-sh/cargo-dist). To install it, go to the releases page and install the binary.

Updating `dist` can be done with `dist selfupdate`.

### Releasing
In order to tag a new release, follow these steps:
- Checkout master
- Add a changelog for the version to `RELEASES.md`
- Commit the changes
- Run `cargo release patch --no-publish` to dry run the release (minor or major can be used as well)
- Run `cargo release patch --no-publish --execute` to actually tag the release
