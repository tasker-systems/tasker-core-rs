{%- extends "base.md" -%}

{% block title %}# {{ section.name }}{% endblock -%}

{% block content %}

**Path:** `{{ section.path }}`
**Context:** {{ context_name }}
{%- match environment %}
{%- when Some with (env) %}
**Environment:** {{ env }}
{%- when None %}
{%- endmatch %}
{%- if !section.description.is_empty() %}

{{ section.description }}
{%- endif %}

---
{%- if !section.parameters.is_empty() %}

## Parameters

| Parameter | Type | Default | Range | Description |
|-----------|------|---------|-------|-------------|
{%- for p in section.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.valid_range }} | {{ p.description }} |
{%- endfor %}
{% for p in section.parameters %}
{%- if p.is_documented %}
{% include "_parameter-detail.md" %}
{% endif %}
{%- endfor %}
{%- endif %}
{%- if !section.subsections.is_empty() %}

## Subsections
{% for sub in section.subsections %}

### {{ sub.name }}

**Path:** `{{ sub.path }}`
{%- if !sub.parameters.is_empty() %}

| Parameter | Type | Default | Range | Description |
|-----------|------|---------|-------|-------------|
{%- for p in sub.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.valid_range }} | {{ p.description }} |
{%- endfor %}
{% for p in sub.parameters %}
{%- if p.is_documented %}
{% include "_parameter-detail.md" %}
{% endif %}
{%- endfor %}
{%- endif %}
{%- for nested in sub.subsections %}

#### {{ nested.name }}

**Path:** `{{ nested.path }}`
{%- if !nested.parameters.is_empty() %}

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
{%- for p in nested.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.description }} |
{%- endfor %}
{%- endif %}
{%- endfor %}
{%- endfor %}
{%- endif %}
{% endblock %}
