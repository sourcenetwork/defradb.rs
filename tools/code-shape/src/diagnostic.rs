use rustc_errors::{Diag, DiagCtxtHandle, Diagnostic, Level};

pub(crate) struct Message {
    pub(crate) text: String,
    pub(crate) help: &'static str,
}

impl<'diagnostic> Diagnostic<'diagnostic, ()> for Message {
    fn into_diag(
        self,
        context: DiagCtxtHandle<'diagnostic>,
        level: Level,
    ) -> Diag<'diagnostic, ()> {
        let mut diagnostic = Diag::new(context, level, self.text);
        diagnostic.help(self.help);
        diagnostic
    }
}
