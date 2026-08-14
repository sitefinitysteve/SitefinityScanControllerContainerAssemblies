// NEGATIVE CONTROL: references the attribute assembly but applies nothing at
// assembly level. Must NOT be reported. Guards against a reader that matches on
// a TypeRef merely being present in the metadata rather than actually applied.
namespace NoAttributes
{
    public class Placeholder
    {
        public string Name { get; set; }
    }
}
