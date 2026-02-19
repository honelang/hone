#!/usr/bin/env python3
"""Generate Hone schema files from Kubernetes JSON Schema definitions.

Downloads _definitions.json from yannh/kubernetes-json-schema and generates
curated Hone schema files for common K8s resource types.

Usage:
    python3 scripts/generate-k8s-schemas.py [--version 1.30] [--output lib/k8s]
"""

import argparse
import json
import os
import sys
import urllib.request
from collections import defaultdict

# Hone reserved words that must be quoted as field names
RESERVED_WORDS = {
    "let", "when", "else", "for", "import", "from", "true", "false", "null",
    "assert", "type", "schema", "variant", "expect", "secret", "policy",
    "deny", "warn", "use", "in", "as", "fn",
}

# Curated list of definitions to include, grouped by output file.
# Keys are output file names, values are lists of K8s definition suffixes
# (after "io.k8s.api." or "io.k8s.apimachinery.").
CURATED = {
    "_meta": [
        "pkg.apis.meta.v1.ObjectMeta",
        "pkg.apis.meta.v1.LabelSelector",
        "pkg.apis.meta.v1.LabelSelectorRequirement",
    ],
    "core": [
        "core.v1.PodSpec",
        "core.v1.PodTemplateSpec",
        "core.v1.Container",
        "core.v1.ContainerPort",
        "core.v1.EnvVar",
        "core.v1.EnvVarSource",
        "core.v1.VolumeMount",
        "core.v1.Volume",
        "core.v1.ConfigMapVolumeSource",
        "core.v1.SecretVolumeSource",
        "core.v1.EmptyDirVolumeSource",
        "core.v1.PersistentVolumeClaimVolumeSource",
        "core.v1.HostPathVolumeSource",
        "core.v1.ResourceRequirements",
        "core.v1.Probe",
        "core.v1.HTTPGetAction",
        "core.v1.TCPSocketAction",
        "core.v1.ExecAction",
        "core.v1.SecurityContext",
        "core.v1.ServiceSpec",
        "core.v1.ServicePort",
        "core.v1.ConfigMap",
        "core.v1.Secret",
        "core.v1.Service",
        "core.v1.PersistentVolumeClaim",
        "core.v1.PersistentVolumeClaimSpec",
        "core.v1.Pod",
        "core.v1.Namespace",
        "core.v1.ServiceAccount",
        "core.v1.KeyToPath",
        "core.v1.ObjectFieldSelector",
        "core.v1.ConfigMapKeySelector",
        "core.v1.SecretKeySelector",
        "core.v1.ResourceFieldSelector",
    ],
    "apps": [
        "apps.v1.Deployment",
        "apps.v1.DeploymentSpec",
        "apps.v1.DaemonSet",
        "apps.v1.DaemonSetSpec",
        "apps.v1.StatefulSet",
        "apps.v1.StatefulSetSpec",
        "apps.v1.ReplicaSet",
        "apps.v1.ReplicaSetSpec",
    ],
    "batch": [
        "batch.v1.Job",
        "batch.v1.JobSpec",
        "batch.v1.CronJob",
        "batch.v1.CronJobSpec",
        "batch.v1.JobTemplateSpec",
    ],
    "networking": [
        "networking.v1.Ingress",
        "networking.v1.IngressSpec",
        "networking.v1.IngressRule",
        "networking.v1.HTTPIngressRuleValue",
        "networking.v1.HTTPIngressPath",
        "networking.v1.IngressBackend",
        "networking.v1.IngressServiceBackend",
        "networking.v1.ServiceBackendPort",
        "networking.v1.IngressTLS",
        "networking.v1.NetworkPolicy",
        "networking.v1.NetworkPolicySpec",
        "networking.v1.NetworkPolicyIngressRule",
        "networking.v1.NetworkPolicyEgressRule",
        "networking.v1.NetworkPolicyPeer",
        "networking.v1.NetworkPolicyPort",
    ],
    "rbac": [
        "rbac.v1.Role",
        "rbac.v1.ClusterRole",
        "rbac.v1.RoleBinding",
        "rbac.v1.ClusterRoleBinding",
        "rbac.v1.PolicyRule",
        "rbac.v1.RoleRef",
        "rbac.v1.Subject",
    ],
}

# Map from full K8s definition key to short Hone schema name
def schema_name(full_key):
    """Convert io.k8s.api.apps.v1.Deployment -> Deployment"""
    return full_key.rsplit(".", 1)[-1]

def safe_field(name):
    """Quote reserved words for body key-value pairs."""
    if name in RESERVED_WORDS:
        return f'"{name}"'
    return name

def schema_field_name(name):
    """Format a field name for use inside a schema definition.

    Hone reserved words must be quoted in schema field names.
    """
    if name in RESERVED_WORDS:
        return f'"{name}"'
    return name

def resolve_ref(ref_str, included_names):
    """Resolve a $ref to a Hone schema name, or 'object' if not included."""
    # "#/definitions/io.k8s.api.apps.v1.Deployment"
    full_key = ref_str.replace("#/definitions/", "")
    name = schema_name(full_key)
    if name in included_names:
        return name
    return "object"

def extract_type(type_val):
    """Extract type from string or nullable array form."""
    if isinstance(type_val, str):
        return type_val
    if isinstance(type_val, list):
        non_null = [t for t in type_val if t != "null"]
        if len(non_null) == 1:
            return non_null[0]
    return None

def map_type(prop_schema, included_names):
    """Convert a JSON Schema property to a Hone type string."""
    # $ref
    if "$ref" in prop_schema:
        return resolve_ref(prop_schema["$ref"], included_names)

    # allOf - take first
    if "allOf" in prop_schema:
        items = prop_schema["allOf"]
        if items:
            return map_type(items[0], included_names)

    # oneOf/anyOf - check for IntOrString pattern
    for key in ("oneOf", "anyOf"):
        if key in prop_schema:
            types = []
            for item in prop_schema[key]:
                t = extract_type(item.get("type"))
                if t:
                    types.append(t)
            non_null = [t for t in types if t != "null"]
            if set(non_null) == {"string", "integer"}:
                return "IntOrString"
            if len(non_null) == 1:
                return TYPE_MAP.get(non_null[0], non_null[0])
            return "object"

    # Direct type
    raw_type = prop_schema.get("type")
    type_str = extract_type(raw_type)

    if type_str == "string":
        return "string"
    elif type_str == "integer":
        fmt = prop_schema.get("format", "")
        if fmt == "int32":
            return "int"
        elif fmt == "int64":
            return "int"
        return "int"
    elif type_str == "number":
        return "float"
    elif type_str == "boolean":
        return "bool"
    elif type_str == "array":
        items = prop_schema.get("items", {})
        item_type = map_type(items, included_names)
        return f"array # {item_type}"
    elif type_str == "object":
        return "object"
    elif type_str is None:
        # No type - check for properties
        if "properties" in prop_schema:
            return "object"
        return "object"

    return "object"

TYPE_MAP = {
    "string": "string",
    "integer": "int",
    "number": "float",
    "boolean": "bool",
    "object": "object",
    "array": "array",
}


def generate_schema(name, definition, included_names):
    """Generate a single Hone schema definition."""
    props = definition.get("properties", {})
    required = set(definition.get("required", []))

    if not props:
        return None

    lines = [f"schema {name} {{"]
    # Sort properties alphabetically, quoting reserved words
    for prop_name in sorted(props.keys()):
        prop_schema = props[prop_name]
        type_str = map_type(prop_schema, included_names)
        field = schema_field_name(prop_name)
        opt = "" if prop_name in required else "?"
        lines.append(f"  {field}{opt}: {type_str}")

    lines.append("  ...")
    lines.append("}")
    return "\n".join(lines)


def build_full_key_map(definitions):
    """Build map from short suffix to full definition key."""
    result = {}
    for key in definitions:
        # io.k8s.api.apps.v1.Deployment -> apps.v1.Deployment
        for prefix in ("io.k8s.api.", "io.k8s.apimachinery."):
            if key.startswith(prefix):
                suffix = key[len(prefix):]
                result[suffix] = key
                break
    return result


def find_dependencies(definition, all_defs, included_names):
    """Find which included schemas this definition references."""
    deps = set()
    props = definition.get("properties", {})
    for prop_schema in props.values():
        if "$ref" in prop_schema:
            ref_name = schema_name(prop_schema["$ref"].replace("#/definitions/", ""))
            if ref_name in included_names:
                deps.add(ref_name)
        if "items" in prop_schema and "$ref" in prop_schema.get("items", {}):
            ref_name = schema_name(prop_schema["items"]["$ref"].replace("#/definitions/", ""))
            if ref_name in included_names:
                deps.add(ref_name)
    return deps


def generate_types_file():
    """Generate the _types.hone with shared type aliases."""
    return """# Kubernetes shared type aliases
#
# Common constrained types used across K8s resource schemas.
# Import these in your .hone files alongside the resource schemas.

type IntOrString = string
type Quantity = string
type K8sName = string(1, 253)
type K8sDnsLabel = string(1, 63)
"""


def generate_file(file_name, suffixes, definitions, key_map, all_included_names, file_schemas):
    """Generate a single .hone schema file."""
    lines = []
    lines.append(f"# Kubernetes {file_name} schemas (auto-generated)")
    lines.append(f"#")
    lines.append(f"# Generated from kubernetes-json-schema v1.30.")
    lines.append(f"# Do not edit manually -- regenerate with scripts/generate-k8s-schemas.py")
    lines.append("")

    # Determine which files we need to import
    imports_needed = set()
    schemas_in_this_file = set()
    for suffix in suffixes:
        full_key = key_map.get(suffix)
        if full_key and full_key in definitions:
            schemas_in_this_file.add(schema_name(full_key))

    for suffix in suffixes:
        full_key = key_map.get(suffix)
        if not full_key or full_key not in definitions:
            continue
        definition = definitions[full_key]
        deps = find_dependencies(definition, definitions, all_included_names)
        for dep in deps:
            if dep not in schemas_in_this_file and dep != "IntOrString":
                # Find which file contains this schema
                for other_file, other_schemas in file_schemas.items():
                    if dep in other_schemas and other_file != file_name:
                        imports_needed.add(other_file)
                        break

    # Always import _types if we reference IntOrString
    needs_types = False
    for suffix in suffixes:
        full_key = key_map.get(suffix)
        if not full_key or full_key not in definitions:
            continue
        definition = definitions[full_key]
        for prop_schema in definition.get("properties", {}).values():
            for key in ("oneOf", "anyOf"):
                if key in prop_schema:
                    types = []
                    for item in prop_schema[key]:
                        t = extract_type(item.get("type"))
                        if t:
                            types.append(t)
                    non_null = [t for t in types if t != "null"]
                    if set(non_null) == {"string", "integer"}:
                        needs_types = True

    if needs_types:
        imports_needed.add("_types")

    # Write imports
    for imp in sorted(imports_needed):
        lines.append(f'import "./{imp}.hone" as {imp.lstrip("_")}')
    if imports_needed:
        lines.append("")

    # Write schemas
    schemas_written = []
    for suffix in suffixes:
        full_key = key_map.get(suffix)
        if not full_key or full_key not in definitions:
            print(f"  WARNING: {suffix} not found in definitions", file=sys.stderr)
            continue
        name = schema_name(full_key)
        definition = definitions[full_key]
        schema_str = generate_schema(name, definition, all_included_names)
        if schema_str:
            if schemas_written:
                lines.append("")
            lines.append(schema_str)
            schemas_written.append(name)

    lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Generate Hone K8s schema library")
    parser.add_argument("--version", default="1.30", help="K8s version (default: 1.30)")
    parser.add_argument("--output", default="lib/k8s", help="Output directory (default: lib/k8s)")
    parser.add_argument("--definitions", help="Path to local _definitions.json (skip download)")
    args = parser.parse_args()

    version = args.version
    minor = f"v{version}.0"
    out_dir = os.path.join(args.output, f"v{version}")

    # Load definitions
    if args.definitions:
        print(f"Loading definitions from {args.definitions}")
        with open(args.definitions) as f:
            data = json.load(f)
    else:
        url = f"https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/{minor}/_definitions.json"
        print(f"Downloading {url}")
        with urllib.request.urlopen(url) as resp:
            data = json.loads(resp.read())

    definitions = data["definitions"]
    print(f"Loaded {len(definitions)} definitions")

    # Build key map
    key_map = build_full_key_map(definitions)

    # Collect all included schema names
    all_included_names = {"IntOrString", "Quantity", "K8sName", "K8sDnsLabel"}
    file_schemas = {}
    for file_name, suffixes in CURATED.items():
        schemas = set()
        for suffix in suffixes:
            full_key = key_map.get(suffix)
            if full_key:
                name = schema_name(full_key)
                all_included_names.add(name)
                schemas.add(name)
        file_schemas[file_name] = schemas

    # Create output directory
    os.makedirs(out_dir, exist_ok=True)

    # Generate _types.hone
    types_content = generate_types_file()
    types_path = os.path.join(out_dir, "_types.hone")
    with open(types_path, "w") as f:
        f.write(types_content)
    print(f"  wrote {types_path}")

    # Generate each file
    total_schemas = 0
    for file_name, suffixes in CURATED.items():
        content = generate_file(
            file_name, suffixes, definitions, key_map,
            all_included_names, file_schemas
        )
        file_path = os.path.join(out_dir, f"{file_name}.hone")
        with open(file_path, "w") as f:
            f.write(content)
        count = content.count("schema ")
        total_schemas += count
        print(f"  wrote {file_path} ({count} schemas)")

    print(f"\nDone: {total_schemas} schemas in {out_dir}/")


if __name__ == "__main__":
    main()
