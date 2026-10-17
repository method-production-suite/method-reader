# Emailing scripts to your Kindle

Amazon lets you email documents to your Kindle, and they arrive wirelessly.
It is one of two routes that keep a script's shape: Amazon converts what you
send with its modern typesetting, so a scene heading or a character name stays
on the same page as the line after it. The other is copying a KFX file over
USB, which needs Calibre, Kindle Previewer and the KFX plugin on your
computer. Without those, a Kindle over USB gets an AZW3 that Method Reader makes
itself (beta), with nothing else installed; it, and the MOBI for the oldest
Kindles, can strand a heading or a name at the foot of a page. Email needs no
tools at all.

Email is also the route for newer Kindles that never appear as a drive when
plugged in, so there is nothing to copy onto.

The trade: email goes through Amazon, USB never leaves your desk. See
[What Amazon receives](#what-amazon-receives).

The setup happens once on Amazon's side, and it is confusing the first time.
**Both steps are required.** Most people do the first, skip the second, and
then their scripts silently never arrive.

Everything below is at Amazon → **Manage Your Content and Devices** →
**Preferences** → **Personal Document Settings**
([direct link](https://www.amazon.com/hz/mycd/myx#/home/settings/pdoc)). In
Method Reader, the email row under **Other ways** on the result screen has
**Open Amazon's page**, which opens that same page in your browser.

## 1. Find your Kindle's own email address

Under *Send-to-Kindle E-Mail Settings*, each device has an address like
`yourname_a1b2c3@kindle.com`. That's where you send scripts. You can edit the
part before the `@` to something memorable. Method Reader does not keep this
address: you type it into the message yourself (see below).

## 2. Approve the address you send *from*

**This is the step everyone misses.**

Under *Approved Personal Document E-Mail List*, click **Add a new approved
e-mail address** and add your own everyday email, the one you'll be sending
from.

Amazon **silently discards** documents from any address not on this list. No
bounce, no error, no message. If your scripts never turn up, this is almost
always why.

## Then send it

Attach the EPUB to a normal email addressed to your `@kindle.com` address and
send it. Subject and body don't matter. It lands on every Kindle on the
account, usually within a minute or two.

On a Mac whose default mail app is Apple Mail, **Send to Kindle email**
under **Other ways** on the result screen does the attaching: it opens a new
Mail message with the book's EPUB already in it, and you add your
`@kindle.com` address and send. Anywhere else, the EPUB row's **Save…** in
the same list saves a copy you can attach from any mail app.

> Send the **EPUB**, not the MOBI or AZW3: Amazon does not accept those by
> email. Each file under **Save a file** says who it is for: EPUB is the one
> for email; KFX, AZW3 and MOBI are for copying to a Kindle by hand over
> USB.

## What Amazon receives

You are handing your script to a third party. Amazon receives the file and
their terms apply to it, not ours. That is your call to make, and Method Reader
never makes it for you: see
[Your script stays on your machine](../README.md#your-script-stays-on-your-machine).
If the material is confidential, USB is the route that never touches a
network.
