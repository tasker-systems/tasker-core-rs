{%- extends "base.md" -%}

{% block title %}# Configuration Reference: {{ context_name }}{% endblock -%}

{% block content %}

> {{ documented_parameters }}/{{ total_parameters }} parameters documented
> Generated: {{ generation_timestamp }}

---
{% for section in sections %}

## {{ section.name }}
{%- if !section.description.is_empty() %}

{{ section.description }}
{%- endif %}

**Path:** `{{ section.path }}`
{%- if !section.parameters.is_empty() %}

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
{%- for p in section.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.description }} |
{%- endfor %}
{% for p in section.parameters %}
{%- if p.is_documented %}
{% include "_parameter-detail.md" %}
{% endif %}
{%- endfor %}
{%- endif %}
{%- for sub in section.subsections %}

### {{ sub.name }}

**Path:** `{{ sub.path }}`
{%- if !sub.description.is_empty() %}

{{ sub.description }}
{%- endif %}
{%- if !sub.parameters.is_empty() %}

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
{%- for p in sub.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.description }} |
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
{% for p in nested.parameters %}
{%- if p.is_documented %}
{% include "_parameter-detail.md" %}
{% endif %}
{%- endfor %}
{%- endif %}
{%- for deep in nested.subsections %}

##### {{ deep.name }}

**Path:** `{{ deep.path }}`
{%- if !deep.parameters.is_empty() %}

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
{%- for p in deep.parameters %}
| `{{ p.name }}` | `{{ p.rust_type }}` | `{{ p.default_value }}` | {{ p.description }} |
{%- endfor %}
{%- endif %}
{%- endfor %}
{%- endfor %}
{%- endfor %}
{%- endfor %}
{% endblock %}
