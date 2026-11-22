pub(crate) fn execute(args: &[String]) {
    if let [flag, name] = args {
        if flag == "-p" {
            println!("declare: {name}: not found");
        }
    }
}
