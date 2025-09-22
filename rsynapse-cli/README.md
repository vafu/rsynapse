
*rsynapse-cli*

`rsynapse-cli` is a command-line interface for interacting with the rsynapse daemon. It allows you to perform searches and execute commands directly from your terminal.

**Usage**
The CLI operates through subcommands. The two primary commands are search and exec.
To search for an item, use the search subcommand followed by your query. The output will be a formatted table of results, including the ID required for execution.


`rsynapse-cli search <QUERY>`

Example:

```
$ rsynapse-cli search fire
+--------------------------------+---------+--------------------------+
| ID                             | Title   | Description              |
+================================+=========+==========================+
| Application Launcher::firefox.desktop | Firefox | Browse the World Wide Web|
+--------------------------------+---------+--------------------------+
```

**Executing an Item (WIP)**

To execute an item, use the exec subcommand followed by the item's unique ID obtained from a search.

`rsynapse-cli exec <ID>`

Example:

```
$ rsynapse-cli exec "Application Launcher::firefox.desktop"
Execution request sent for ID: Application Launcher::firefox.desktop
```

