pub(crate) fn execute_complete(args: &[String]) {
    if args.first().map(String::as_str) != Some("-p") || args.len() != 2 {
        return;
    }

    println!("complete: {}: no completion specification", args[1]);
}
