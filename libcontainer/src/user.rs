pub struct User {
    name: String,
    pass: String,
    uid: i32,
    gid: i32,
    gecos: String,
    home: String,
    shell: String,
}

pub struct Group {
	name: String,
	pass: String,
	gid: i32,
	list: Vec<String>,
}

pub struct SubID {
	name: String,
	subid: i64,
	count: i64,
}

pub struct IDMap {
	id: i64,
	parent_id: i64,
	count: i64,
}

pub struct ExecUser {
	uid: i32,
	gid: i32,
	sgids: Vec<i32>,
	home: String,
}

