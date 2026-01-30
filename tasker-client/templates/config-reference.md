# Configuration Reference: {{ context_name }}

> {{ documented_parameters }}/{{ total_parameters }} parameters documented
> Generated: {{ generation_timestamp }}

---
{% for section in sections %}

## {{ section.name }}
{% if !section.description.is_empty() %}

{{ section.description }}
{% endif %}

**Path:** `{{ section.path }}`
{% if !section.parameters.is_empty() %}

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
{% for p in section.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.description }} |
{% endfor %}
{% for p in section.parameters %}
{% if p.is_documented %}

### `{{ p.path }}`

{{ p.description }}

- **Type:** `{{ p.rust_type }}`
- **Default:** `{{ p.default_value }}`
{% if !p.valid_range.is_empty() %}
- **Valid Range:** {{ p.valid_range }}
{% endif %}
{% if !p.system_impact.is_empty() %}
- **System Impact:** {{ p.system_impact }}
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
{% if !p.example.is_empty() %}

**Example:**
```toml
{{ p.example }}
```
{% endif %}
{% endif %}
{% endfor %}
{% endif %}
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
{% for nested in sub.subsections %}

#### {{ nested.name }}

**Path:** `{{ nested.path }}`
{% if !nested.parameters.is_empty() %}

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
{% for p in nested.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.description }} |
{% endfor %}
{% endif %}
{% endfor %}
{% endfor %}
{% endfor %}
