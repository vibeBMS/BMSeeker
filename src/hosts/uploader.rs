//! Dead uploaders whose files are kept elsewhere by number
//! ([`Config::uploaders`]): a link to one goes on to its mirrors, in order.
//!
//! [`Config::uploaders`]: crate::config::Config::uploaders

use super::{Host, Link, Resolved, Want};
use crate::config::{Config, Uploader};
use crate::net::{BoxTask, Ready};

pub(crate) struct Uploaders(pub Vec<Uploader>);

impl Host for Uploaders {
    fn name(&self) -> &'static str {
        "uploader"
    }

    fn accepts(&self, link: &Link) -> bool {
        self.0.iter().any(|u| u.number(&link.url).is_some())
    }

    fn open(&self, link: &Link, _: Option<&Want>, _: &Config) -> BoxTask<Resolved> {
        let mirrors: Vec<String> = self.0.iter().flat_map(|u| u.mirrored(&link.url)).collect();
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
    use crate::hosts::{Context, Target};
    use crate::net::{run, Canned};

    #[test]
    fn to_the_mirrors() {
        assert_ne!(Context::new(Config::default()).hosts.names()[0], "uploader");
        let cx = Context::new(Config {
            third_party: true,
            ..Config::default()
        });
        assert_eq!(cx.hosts.names()[0], "uploader");
        let http = Canned(vec![]);
        let mut r = crate::hosts::Resolver::new(
            "http://gnqg.rosx.net/upload/upload.cgi?get=03691",
            None,
            &cx,
        );
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
