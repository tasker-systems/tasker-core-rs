
#### `{{ p.path }}`

{{ p.description }}

- **Type:** `{{ p.rust_type }}`
- **Default:** `{{ p.default_value }}`
{%- if !p.valid_range.is_empty() %}
- **Valid Range:** {{ p.valid_range }}
{%- endif %}
{%- if !p.system_impact.is_empty() %}
- **System Impact:** {{ p.system_impact }}
{%- endif %}
{%- if !p.recommendations.is_empty() %}

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
{%- for rec in p.recommendations %}
| {{ rec.environment }} | {{ rec.value }} | {{ rec.rationale }} |
{%- endfor %}
{%- endif %}
{%- if !p.related.is_empty() %}

**Related:** {% for r in p.related %}`{{ r }}`{% if !loop.last %}, {% endif %}{% endfor %}
{%- endif %}
{%- if !p.example.is_empty() %}

**Example:**
```toml
{{ p.example }}
```
{%- endif %}
