// Stand-ins for the real Feather attributes, under the exact namespace and
// type names sfscan matches on. Declaring them here means the consuming
// fixtures reference them across an assembly boundary, which is what produces
// the MemberReference -> TypeReference resolution path in the metadata.
namespace Telerik.Sitefinity.Frontend.Mvc.Infrastructure.Controllers.Attributes
{
    [System.AttributeUsage(System.AttributeTargets.Assembly, AllowMultiple = false)]
    public sealed class ControllerContainerAttribute : System.Attribute
    {
    }

    [System.AttributeUsage(System.AttributeTargets.Assembly, AllowMultiple = true)]
    public sealed class ResourcePackageAttribute : System.Attribute
    {
        public ResourcePackageAttribute()
        {
        }

        public ResourcePackageAttribute(string name)
        {
            this.Name = name;
        }

        public string Name { get; }
    }
}
