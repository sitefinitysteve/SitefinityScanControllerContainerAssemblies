// POSITIVE: attribute lives in another assembly, so its constructor resolves
// through CustomAttributeType -> MemberRef -> MemberRefParent -> TypeRef.
// This is the common case for real Sitefinity widget assemblies.
[assembly: Telerik.Sitefinity.Frontend.Mvc.Infrastructure.Controllers.Attributes.ControllerContainer]
