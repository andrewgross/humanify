//! The package families: explicit families, the same-scope rule and the
//! scoped forks.

use crate::libindex::families::PackageFamilies;

#[test]
fn families_follow_the_shipped_rules() {
    let f = PackageFamilies::shipped();
    // An explicit family, with a prefix member.
    assert_eq!(
        f.family("@aws-sdk/client-s3"),
        f.family("@smithy/node-http-handler")
    );
    assert_eq!(f.family("lodash.merge"), f.family("es-toolkit"));
    assert_eq!(f.family("minimatch"), f.family("glob"));
    // Two packages of one scope are one family.
    assert_eq!(
        f.family("@anthropic-ai/sdk"),
        f.family("@anthropic-ai/sandbox-runtime")
    );
    assert_eq!(
        f.family("@azure/identity"),
        f.family("@azure/core-rest-pipeline")
    );
    // A scoped fork is its original's family.
    assert_eq!(f.family("@colors/colors"), f.family("colors"));
    // Unrelated packages are not.
    assert_ne!(f.family("react"), f.family("react-dom"));
    assert_ne!(f.family("@aws-sdk/client-s3"), f.family("@azure/identity"));
    assert_eq!(f.family("ink"), "ink");
}
