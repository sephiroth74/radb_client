pub trait AsArgs<S> {
	fn as_args(&self) -> Vec<S>;
}

pub trait AsArg<S> {
	fn as_arg(&self) -> S;
}
