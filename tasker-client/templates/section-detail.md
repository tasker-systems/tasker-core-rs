# {{ section.name }}

**Path:** `{{ section.path }}`
**Context:** {{ context_name }}
{% if environment.is_some() %}
**Environment:** {{ environment.unwrap() }}
{% endif %}
{% if !section.description.is_empty() %}

{{ section.description }}
{% endif %}

---
{% if !section.parameters.is_empty() %}

## Parameters

| Parameter | Type | Default | Range | Description |
|-----------|------|---------|-------|-------------|
{% for p in section.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.valid_range }} | {{ p.description }} |
{% endfor %}
{% for p in section.parameters %}
{% if p.is_documented %}

### `{{ p.name }}`

{{ p.description }}

- **Full Path:** `{{ p.path }}`
- **Type:** `{{ p.rust_type }}`
- **Default:** `{{ p.default_value }}`
{% if !p.valid_range.is_empty() %}
- **Valid Range:** {{ p.valid_range }}
{% endif %}
{% if !p.system_impact.is_empty() %}

**System Impact:** {{ p.system_impact }}
{% endif %}
{% if !p.recommendations.is_empty() %}

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
{% for rec in p.recommendations %}
| {{ rec.environment }} | {{ rec.value }} | {{ rec.rationale }} |
{% endfor %}
{% endif %}
{% if !p.related.is_empty() %}

**Related:** {% for r in p.related %}`{{ r }}`{% if !loop.last %}, {% endif %}{% endfor %}
{% endif %}
{% endif %}
{% endfor %}
{% endif %}
{% if !section.subsections.is_empty() %}

## Subsections
{% for sub in section.subsections %}

### {{ sub.name }}

**Path:** `{{ sub.path }}`
{% if !sub.parameters.is_empty() %}

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
{% for p in sub.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.description }} |
{% endfor %}
{% endif %}
{% endfor %}
{% endif %}
