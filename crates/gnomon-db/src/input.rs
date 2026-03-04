use std::fmt;
use std::path::PathBuf;

#[salsa::input]
pub struct SourceFile {
    #[returns(ref)]
    pub path: PathBuf,
    #[returns(ref)]
    pub text: String,
}

impl fmt::Debug for SourceFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id: salsa::Id = salsa::plumbing::AsId::as_id(self);
        f.debug_tuple("SourceFile").field(&id).finish()
    }
}
