# Chuva

Rain prediction for The Netherlands

* Live: <https://chuva.caio.co>
* Demo: <https://chuva.caio.co/demo>
* Git Repository: <https://caio.co/de/chuva/>

## History

Earlier this year (2025) I got fed up with how slow and full of ads the
service I used before got, then I found out that [KNMI][] publishes
precipitation predictions so I quickly hacked something that dumped what
wanted to the terminal

For months, this is all that chuva was: a script downloading the most
recent dataset and me ssh'ing into the server to call the program. Then
came October with a lot of random rain that barely gets you wet which
prompted me to make it easier to access it.

I kept the support for plain-text predictions since I like to avoid
going to the browser to prevent distractions. You can use it too if
you like, either via the accept header or a `txt=1` query string:

```
$ curl -H accept:text/plain https://chuva.caio.co/demo
It's 10:48

▃▄ ▄▆▆▅▁          ▁▄▅▄▂
^
- Rain ends in 36 minutes
- Rain from 12:15 until 12:40
```

[KNMI]: https://knmi.nl

## Project Layout

In alphabetical order:

- `caveman`: A `hyper`-based, `serde`-less http1 thing that can answer
  traditional web requests and knows to shutdown gracefully
- `chuva`: An API for interacting with the KNMI dataset. It loads the
  dataset to RAM in a format that makes it super easy to answer the
  "what's the forecast for a given coordinate" question
- `download-dataset`: The "cron job" that retrieves the most recent
  dataset
- `etc/systemd`: A skeleton of the systemd unit files that drive the service
- `moros`: The web server, front-end for chuva.caio.co. Designed to be
  killed/restarted periodically to pick up the most recent data
- `postcode-fst`: Code to generate the compact dictionary with dutch post
  codes used by `moros`

## License

This software is licensed under the [European Union Public License
(EUPL) v. 1.2 only][EUPL-1.2]

[EUPL-1.2]: https://joinup.ec.europa.eu/collection/eupl/eupl-text-eupl-12

