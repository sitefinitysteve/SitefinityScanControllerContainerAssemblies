// POSITIVE, harder path: the attribute is declared in THIS assembly and applied
// here, so its constructor resolves through CustomAttributeType -> MethodDef.
// Reaching the type name then requires walking TypeDef.MethodList ranges to find
// the declaring type, which is the trickiest part of the metadata reader.
//
// C# requires assembly-level attributes to precede any other declarations, so
// the attribute application sits above the namespace that defines it. That is
// legal: the type does not need to be declared earlier in the file.
[assembly: Telerik.Sitefinity.Frontend.Mvc.Infrastructure.Controllers.Attributes.ResourcePackage("SelfDefinedPackage")]

namespace Telerik.Sitefinity.Frontend.Mvc.Infrastructure.Controllers.Attributes
{
    [System.AttributeUsage(System.AttributeTargets.Assembly)]
    public sealed class ResourcePackageAttribute : System.Attribute
    {
        public ResourcePackageAttribute(string name)
        {
            this.Name = name;
        }

        public string Name { get; }
    }
}
