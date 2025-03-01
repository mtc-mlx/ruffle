use crate::avm2::Error;
use crate::context::UpdateContext;
use crate::string::{AvmAtom, AvmString};
use crate::{avm2::script::TranslationUnit, context::GcContext};
use gc_arena::{Collect, Gc, Mutation};
use num_traits::FromPrimitive;
use ruffle_wstr::WStr;
use std::fmt::Debug;
use swf::avm2::types::{Index, Namespace as AbcNamespace};

use super::api_version::ApiVersion;

#[derive(Clone, Copy, Collect, Debug)]
#[collect(no_drop)]
pub struct Namespace<'gc>(Gc<'gc, NamespaceData<'gc>>);

/// Represents the name of a namespace.
#[allow(clippy::enum_variant_names)]
#[derive(Copy, Clone, Collect, Debug, PartialEq, Eq)]
#[collect(no_drop)]
enum NamespaceData<'gc> {
    // note: this is the default "public namespace", corresponding to both
    // ABC Namespace and PackageNamespace
    Namespace(AvmAtom<'gc>, #[collect(require_static)] ApiVersion),
    PackageInternal(AvmAtom<'gc>),
    Protected(AvmAtom<'gc>),
    Explicit(AvmAtom<'gc>),
    StaticProtected(AvmAtom<'gc>),
    // note: private namespaces are always compared by pointer identity
    // of the enclosing `Gc`.
    Private(AvmAtom<'gc>),
    Any,
}

fn strip_version_mark(url: &WStr) -> Option<(&WStr, ApiVersion)> {
    // See https://github.com/adobe/avmplus/blob/858d034a3bd3a54d9b70909386435cf4aec81d21/core/AvmCore.h#L485
    const MIN_API_MARK: usize = 0xE000;
    const MAX_API_MARK: usize = 0xF8FF;

    const WEIRD_START_MARK: usize = 0xE294;

    if let Some(Ok(chr)) = url.chars().last() {
        let chr = chr as usize;
        if chr >= MIN_API_MARK && chr <= MAX_API_MARK {
            // Note - sometimes asc.jar emits a version mark of 0xE000.
            // We treat this as `AllVersions`
            let version = if chr >= WEIRD_START_MARK {
                // Note that is an index into the ApiVersion enum, *not* an SWF version
                ApiVersion::from_usize(chr - WEIRD_START_MARK)
                    .unwrap_or_else(|| panic!("Bad version mark {chr}"))
            } else {
                ApiVersion::AllVersions
            };

            // Since we had a char in the range 0xE000-0xF8FF, we must
            // have a wide WStr, so we can remove this char by just
            // removing the last byte
            assert!(url.is_wide());
            let stripped = &url[..url.len() - 1];
            return Some((stripped, version));
        }
    }
    None
}

impl<'gc> Namespace<'gc> {
    /// Read a namespace declaration from the ABC constant pool and copy it to
    /// a namespace value.
    /// NOTE: you should use the TranslationUnit.pool_namespace instead of calling this.
    /// otherwise you run a risk of creating a duplicate of private ns singleton.
    /// Based on https://github.com/adobe/avmplus/blob/858d034a3bd3a54d9b70909386435cf4aec81d21/core/AbcParser.cpp#L1459
    pub fn from_abc_namespace(
        translation_unit: TranslationUnit<'gc>,
        namespace_index: Index<AbcNamespace>,
        context: &mut UpdateContext<'_, 'gc>,
    ) -> Result<Self, Error<'gc>> {
        if namespace_index.0 == 0 {
            return Ok(Self::any(context.gc_context));
        }

        let actual_index = namespace_index.0 as usize - 1;
        let abc = translation_unit.abc();
        let abc_namespace: Result<_, Error<'gc>> = abc
            .constant_pool
            .namespaces
            .get(actual_index)
            .ok_or_else(|| format!("Unknown namespace constant {}", namespace_index.0).into());
        let abc_namespace = abc_namespace?;

        let index = match abc_namespace {
            AbcNamespace::Namespace(idx)
            | AbcNamespace::Package(idx)
            | AbcNamespace::PackageInternal(idx)
            | AbcNamespace::Protected(idx)
            | AbcNamespace::Explicit(idx)
            | AbcNamespace::StaticProtected(idx)
            | AbcNamespace::Private(idx) => idx,
        };

        let mut namespace_name = translation_unit.pool_string(index.0, &mut context.borrow_gc())?;

        // Private namespaces don't get any of the namespace version checks
        if let AbcNamespace::Private(_) = abc_namespace {
            return Ok(Self(Gc::new(
                context.gc_context,
                NamespaceData::Private(namespace_name),
            )));
        }

        // FIXME - what other versioned urls are there?
        let is_versioned_url = |url: AvmAtom<'gc>| url.as_wstr().is_empty();

        let is_public = matches!(
            abc_namespace,
            AbcNamespace::Namespace(_) | AbcNamespace::Package(_)
        );

        let api_version = if index.0 != 0 {
            let is_playerglobals = translation_unit
                .domain()
                .is_playerglobals_domain(context.avm2);

            let mut api_version = ApiVersion::AllVersions;
            let stripped = strip_version_mark(namespace_name.as_wstr());
            let has_version_mark = stripped.is_some();
            if let Some((stripped, version)) = stripped {
                assert!(
                    is_playerglobals,
                    "Found versioned url {namespace_name:?} in non-playerglobals domain"
                );
                let stripped_string = AvmString::new(context.gc_context, stripped);
                namespace_name = context.interner.intern(context.gc_context, stripped_string);
                api_version = version;
            }

            if is_playerglobals {
                if !has_version_mark && is_public && is_versioned_url(namespace_name) {
                    api_version = ApiVersion::VM_INTERNAL;
                }
            } else if is_public {
                api_version = translation_unit.api_version(context.avm2);
            };
            api_version
        } else {
            // Note - avmplus walks the (user) call stack to determine the API version.
            // However, Flash Player appears to always use the root SWF api version
            // for all swfs (e.g. those loaded through `Loader`). We can simply our code
            // by skipping walking the stack, and just using the API version of our root SWF.
            context.avm2.root_api_version
        };

        let ns = match abc_namespace {
            AbcNamespace::Namespace(_) | AbcNamespace::Package(_) => {
                NamespaceData::Namespace(namespace_name, api_version)
            }
            AbcNamespace::PackageInternal(_) => NamespaceData::PackageInternal(namespace_name),
            AbcNamespace::Protected(_) => NamespaceData::Protected(namespace_name),
            AbcNamespace::Explicit(_) => NamespaceData::Explicit(namespace_name),
            AbcNamespace::StaticProtected(_) => NamespaceData::StaticProtected(namespace_name),
            AbcNamespace::Private(_) => unreachable!(),
        };
        Ok(Self(Gc::new(context.gc_context, ns)))
    }

    pub fn any(mc: &Mutation<'gc>) -> Self {
        Self(Gc::new(mc, NamespaceData::Any))
    }

    // TODO(moulins): allow passing an AvmAtom or a non-static `&WStr` directly
    pub fn package(
        package_name: impl Into<AvmString<'gc>>,
        api_version: ApiVersion,
        context: &mut GcContext<'_, 'gc>,
    ) -> Self {
        let atom = context
            .interner
            .intern(context.gc_context, package_name.into());
        Self(Gc::new(
            context.gc_context,
            NamespaceData::Namespace(atom, api_version),
        ))
    }

    // TODO(moulins): allow passing an AvmAtom or a non-static `&WStr` directly
    pub fn internal(
        package_name: impl Into<AvmString<'gc>>,
        context: &mut GcContext<'_, 'gc>,
    ) -> Self {
        let atom = context
            .interner
            .intern(context.gc_context, package_name.into());
        Self(Gc::new(
            context.gc_context,
            NamespaceData::PackageInternal(atom),
        ))
    }

    pub fn is_public(&self) -> bool {
        matches!(*self.0, NamespaceData::Namespace(name, _) if name.as_wstr().is_empty())
    }

    pub fn is_public_ignoring_ns(&self) -> bool {
        matches!(*self.0, NamespaceData::Namespace(_, _))
    }

    pub fn is_any(&self) -> bool {
        matches!(*self.0, NamespaceData::Any)
    }

    pub fn is_private(&self) -> bool {
        matches!(*self.0, NamespaceData::Private(_))
    }

    pub fn is_namespace(&self) -> bool {
        matches!(*self.0, NamespaceData::Namespace(_, _))
    }

    pub fn as_uri_opt(&self) -> Option<AvmString<'gc>> {
        match *self.0 {
            NamespaceData::Namespace(a, _) => Some(a.into()),
            NamespaceData::PackageInternal(a) => Some(a.into()),
            NamespaceData::Protected(a) => Some(a.into()),
            NamespaceData::Explicit(a) => Some(a.into()),
            NamespaceData::StaticProtected(a) => Some(a.into()),
            NamespaceData::Private(a) => Some(a.into()),
            NamespaceData::Any => None,
        }
    }

    /// Get the string value of this namespace, ignoring its type.
    ///
    /// TODO: Is this *actually* the namespace URI?
    pub fn as_uri(&self) -> AvmString<'gc> {
        self.as_uri_opt().unwrap_or_else(|| "".into())
    }

    /// Compares two namespaces, requiring that their versions match exactly.
    /// Normally, you should use `matches_ns,` which takes version compatibility
    /// into account.
    ///
    /// Namespace does not implement `PartialEq`, so that each caller is required
    /// to explicitly choose either `exact_version_match` or `matches_ns`.
    pub fn exact_version_match(&self, other: Self) -> bool {
        if Gc::as_ptr(self.0) == Gc::as_ptr(other.0) {
            true
        } else if self.is_private() || other.is_private() {
            false
        } else {
            *self.0 == *other.0
        }
    }

    /// Compares this namespace to another, considering them equal if this namespace's version
    /// is less than or equal to the other (definitions in this namespace version can be
    /// seen by the other). This is used to implement `ProperyMap`, where we want to
    /// a definition with `ApiVersion::SWF_16` to be visible when queried from
    /// a SWF with `ApiVersion::SWF_16` or any higher version.
    pub fn matches_ns(&self, other: Self) -> bool {
        if self.exact_version_match(other) {
            return true;
        }
        match (&*self.0, &*other.0) {
            (
                NamespaceData::Namespace(name1, version1),
                NamespaceData::Namespace(name2, version2),
            ) => {
                let name_matches = name1 == name2;
                let version_matches = version1 <= version2;
                if name_matches && !version_matches {
                    tracing::info!(
                        "Rejecting namespace match due to versions: {:?} {:?}",
                        self.0,
                        other.0
                    );
                }
                name_matches && version_matches
            }
            _ => false,
        }
    }
}
