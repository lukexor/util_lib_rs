# Utility Library

This is a small collection of reusable rust utilities for use across projects.

## Performance Profiling

The `performance` module contains methods for profiling CPU and bandwidth
throughput.

Simply call `performance::profile_begin()` when you want to start profiling and
`performance::profile_end_and_print()` to print the results.

Profile individual functions with the `profile!()` macro or blocks with
`profile!("my label")`.

To track bandwidth throughput, pass the number of bytes as a second parameter:
`profile!("read data", bytes_read)`.
