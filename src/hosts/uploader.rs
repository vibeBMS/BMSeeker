//! Dead uploaders whose files are kept elsewhere by number
//! ([`Config::uploaders`]): a link to one goes on to its mirrors, in order,
//! when third parties' copies are used ([`Config::third_party`]); else it
//! fails at once (nothing is asked of a host that is gone).
//!
//! [`Config::uploaders`]: crate::config::Config::uploaders
//! [`Config::third_party`]: crate::config::Config::third_party

use super::{Host, Link, Resolved, Want};
use crate::config::{Config, Uploader};
use crate::net::{BoxTask, Ready};

pub(crate) struct Uploaders {
    pub(crate) uploaders: Vec<Uploader>,
    /// Their mirrors are used (third parties' copies).
    pub(crate) mirrored: bool,
}

impl Host for Uploaders {
    fn name(&self) -> &'static str {
        "uploader"
    }

    fn accepts(&self, link: &Link) -> bool {
        self.uploaders.iter().any(|u| u.number(&link.url).is_some())
    }

    fn open(&self, link: &Link, _: Option<&Want>, _: &Config) -> BoxTask<Resolved> {
        if !self.mirrored {
            return Ready::boxed(Resolved::Failed(
                "the uploader is gone (its mirrors are third parties' copies: off)".into(),
            ));
        }
        let mirrors: Vec<String> = self
            .uploaders
            .iter()
            .flat_map(|u| u.mirrored(&link.url))
            .collect();
        Ready::boxed(if mirrors.is_empty() {
            Resolved::Failed("the uploader is gone and no mirror is set".into())
        } else {
            Resolved::Follow(mirrors)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::hosts::Context;
    use crate::net::job::Target;
    use crate::net::{run, Canned};

    #[test]
    fn to_the_mirrors() {
        let gnqg = "http://gnqg.rosx.net/upload/upload.cgi?get=03691";
        let http = Canned(vec![]);
        // without third parties' copies: gone, nothing asked
        let cx = Context::new(Config::default());
        let mut r = crate::hosts::Resolver::new(gnqg, None, &cx);
        assert!(matches!(
            run(&mut r, &http),
            Err(crate::hosts::Fetch::Failed(crate::net::NetError::Other(e))) if e.contains("third parties")
        ));
        let cx = Context::new(Config {
            third_party: true,
            ..Config::default()
        });
        assert_eq!(cx.hosts.names()[0], "uploader");
        // a table's entry with the uploader's link (filled from no other)
        let e = crate::table::TableEntry {
            md5: "75dc1ff763e736ba307fbc39ec9a386b".into(),
            url: gnqg.into(),
            ..Default::default()
        };
        let entry = crate::install::Entry::from_table(&e, &crate::links::Links::default());
        assert_eq!(entry.song, [gnqg]);
        let mut r = crate::hosts::Resolver::new(&entry.song[0], None, &cx);
        assert_eq!(
            run(&mut r, &http),
            Ok(Target::url(
                "https://bms.hexlataia.xyz/mirror/gnqg-upload/03691.zip"
            ))
        );
        assert!(r.try_next());
        assert_eq!(
            run(&mut r, &http),
            Ok(Target::url(
                "https://darksabun.club/mirror/gnqg-upload/03691.zip"
            ))
        );
        // then the community archive's folder
        assert!(r.try_next());
    }
}
