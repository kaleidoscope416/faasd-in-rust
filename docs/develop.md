# How to develop

Thanks to the `nix`, we can easily develop the project in a reproducible environment. The following steps will guide you through the process.

## nix environment
> TODO

## shell interactions

There are several integrated nix commands that can help you develop the project.

- `nix develop`
    enter the default development shell
- `nix build`
    build the project, the result will be at `result/bin/faas-rs`
- `nix flake check`
    do the CI checks, including formatting, linting...

Besides, you can also use the `cargo` command to interact with the project in the shell. Nix will automatically manage the toolchain and library dependencies for you.

> [!NOTE]
> The project uses `cargo-hakari`, to optimize the build time.

If you don't know what it is, after new dependencies are added, you should run `cargo hakari generate` to update the dependencies.
