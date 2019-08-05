extern crate cc;

fn main () {
	cc::Build::new()
		.file("src/nsenter/nsexec.c")
		.file("src/nsenter/cloned_binary.c")
		.include("src/nsenter")
		.compile("nsenter");
}
