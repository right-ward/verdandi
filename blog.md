---
layout: default
title: Devlog
permalink: /blog/
description: "Development notes from the making of The Ashen Chronicle: its world, gameplay, interface, and ongoing experiments."
---

# Devlog

The Ashen Chronicle is being built in the open. These notes cover the decisions, improvements, experiments, and rough edges that shape the game as it grows.

This is a development log, not a promise that every idea is already in the game. Posts aim to make it clear what has changed, what is still being worked on, and where the project might go next.

Subscribe with the <a href="{{ '/feed.xml' | relative_url }}">Atom feed</a>, or [follow the project on GitHub](https://github.com/right-ward/ashen_chronicle).

{% assign blog_posts = site.posts %}
{% if blog_posts.size > 0 %}
<ul>
{% for post in blog_posts %}
  <li>
    <time datetime="{{ post.date | date_to_xmlschema }}">{{ post.date | date: "%B %-d, %Y" }}</time> — <a href="{{ post.url | relative_url }}">{{ post.title | escape }}</a>
    {% if post.excerpt %}
    <p>{{ post.excerpt | strip_html | normalize_whitespace | truncate: 240 | escape }}</p>
    {% endif %}
  </li>
{% endfor %}
</ul>
{% else %}
<p>No posts yet. New entries will appear here as the project develops.</p>
{% endif %}
